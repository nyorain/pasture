use std::{
    collections::HashMap,
    convert::TryInto,
    io::{Cursor, Seek, Write},
};

use anyhow::{Context, Result};
use las_rs::point;
use pasture_core::{
    containers::{
        PerAttributePointBuffer, PerAttributePointBufferExt, PerAttributeVecPointStorage,
        PointBuffer, PointBufferExt, PointBufferWriteable,
    },
    layout::{
        attributes::{COLOR_RGB, NORMAL, POSITION_3D},
        conversion::{get_converter_for_attributes, AttributeConversionFn},
        FieldAlignment, PointAttributeDataType, PointAttributeDefinition, PointLayout,
    },
    math::Alignable,
};
use serde_json::{json, Value};

use crate::{
    base::PointWriter,
    tiles3d::{ser_batch_table_header, ser_feature_table_header, PntsHeader},
};

use super::{BatchTableHeader, FeatureTableDataReference, FeatureTableHeader, FeatureTableValue};

/// Maximum required alignment
const PNTS_SEMANTICS_MAX_ALIGNMENT: usize = 8;
/// The current .pnts version of 3D Tiles
const PNTS_VERSION: u32 = 1;

/// Returns the corresponding point semantic name for the given `attribute`
fn pnts_semantics_name_from_point_attribute(
    attribute: &PointAttributeDefinition,
) -> Option<String> {
    if attribute.name() == POSITION_3D.name() {
        Some("POSITION".into())
    } else if attribute.name() == COLOR_RGB.name() {
        Some("RGB".into())
    } else if attribute.name() == NORMAL.name() {
        Some("NORMAL".into())
    } else {
        None
    }
}

/// Writer for .pnts files, the point cloud file format in the 3D Tiles standard.
///
/// 3D Tiles .pnts files store their data in per-attribute memory layout. Append to data
/// in per-attribute layout is non-trivial, as attributes are tightly packed (i.e. attribute
/// A is immediately followed by attribute B, with no room to insert new attribute A records).
/// Supporting multiple consecutive `write` operations can be achieved in one of two ways:
/// 1) Upon a write, read the data from the current file and write it, together with the new data,
///    into a new file
/// 2) Cache all data locally in a `PerAttributePointBuffer`, and only write the data during
///    the `flush` call
///
/// This `PntsWriter` implementation uses the second approach
pub struct PntsWriter<W: Write> {
    writer: W,
    expected_layout: PointLayout,
    default_layout: PointLayout,
    cached_points: PerAttributeVecPointStorage,
    attribute_converters: HashMap<&'static str, Option<AttributeConversionFn>>,
    requires_flush: bool,
}

impl<W: Write> PntsWriter<W> {
    /// Creates a new `PntsWriter` writing to the given `writer` and using the given `point_layout`. Please note that
    /// while 3D Tiles does in principle support arbitrary point attributes, currently only the default point semantics
    /// are supported (see [3D Tiles specification](https://github.com/CesiumGS/3d-tiles/blob/master/specification/TileFormats/PointCloud/README.md#semantics)). All further attributes are simply ignored silently!
    pub fn from_write_and_layout(writer: W, point_layout: PointLayout) -> Self {
        // The PntsWriter can accept any kind of point buffer, but it will silently discard attributes that are not
        // supported by 3D Tiles. All supported attributes that are also in `point_layout` are described by `cache_layout`
        let (cache_layout, attribute_converters) = Self::make_compatible_layout(&point_layout);
        let cache = PerAttributeVecPointStorage::new(point_layout.clone());
        Self {
            writer,
            expected_layout: point_layout,
            default_layout: cache_layout,
            cached_points: cache,
            attribute_converters,
            requires_flush: true,
        }
    }

    /// Makes the given `PointLayout` compatible with the supported point semantics of the 3D Tiles .pnts format. Doing
    /// so is done by iterating through the attributes in the `point_layout` and checking each attribute if it is one of
    /// the supported point semantics. If not, it is discarded. Supported semantics are then converted to the default data
    /// type as per the [3D Tiles standard](https://github.com/CesiumGS/3d-tiles/blob/master/specification/TileFormats/PointCloud/README.md#semantics)
    fn make_compatible_layout(
        point_layout: &PointLayout,
    ) -> (
        PointLayout,
        HashMap<&'static str, Option<AttributeConversionFn>>,
    ) {
        let mut compatible_layout = PointLayout::default();
        let mut conversion_fns: HashMap<&'static str, Option<AttributeConversionFn>> =
            HashMap::new();
        // TODO Support for other attributes:
        // * Quantized positions
        // * RGBA colors
        // * RGB565 colors
        // * Normal oct encoded
        // * Batch ID (and batch table with custom attributes)

        let supported_attributes = vec![
            (POSITION_3D.name(), PointAttributeDataType::Vec3f32),
            (COLOR_RGB.name(), PointAttributeDataType::Vec3u8),
            (NORMAL.name(), PointAttributeDataType::Vec3f32),
        ];

        for (attribute_name, attribute_datatype) in supported_attributes.iter() {
            if let Some(src_attribute) = point_layout.get_attribute_by_name(attribute_name) {
                compatible_layout.add_attribute(
                    PointAttributeDefinition::custom(attribute_name, *attribute_datatype),
                    FieldAlignment::Default,
                );
                let target_attribute = compatible_layout
                    .get_attribute_by_name(attribute_name)
                    .unwrap();
                if src_attribute.datatype() == PointAttributeDataType::Vec3f32 {
                    conversion_fns.insert(attribute_name, None);
                } else {
                    conversion_fns.insert(
                        attribute_name,
                        get_converter_for_attributes(
                            &src_attribute.into(),
                            &target_attribute.into(),
                        ),
                    );
                }
            }
        }

        (compatible_layout, conversion_fns)
    }

    fn write_cached_points(&mut self) -> Result<()> {
        let feature_table_header = self.create_feature_table();
        let batch_table_header = self.create_batch_table();

        let mut feature_table_blob = vec![];
        let mut batch_table_blob = vec![];

        ser_feature_table_header(Cursor::new(&mut feature_table_blob), &feature_table_header)
            .context("Error serializing FeatureTable header")?;
        ser_batch_table_header(Cursor::new(&mut batch_table_blob), &batch_table_header)
            .context("Error serializing BatchTable header")?;

        let feature_table_byte_size = feature_table_blob.len();
        let batch_table_byte_size = batch_table_blob.len();
        let feature_table_body_byte_size = self.calc_feature_table_body_length();
        //TODO Support batch table body
        let batch_table_body_byte_size: usize = 0;
        let total_byte_length = PntsHeader::BYTE_LENGTH
            + feature_table_byte_size
            + feature_table_body_byte_size
            + batch_table_byte_size
            + batch_table_body_byte_size;

        let pnts_header = PntsHeader::new(
            PNTS_VERSION,
            total_byte_length
                .try_into()
                .expect("Size of .pnts file exceeds maximum size of 4GiB!"),
            feature_table_byte_size
                .try_into()
                .expect("Size of FeatureTable header exceeds maximum size of 4GiB!"),
            feature_table_body_byte_size
                .try_into()
                .expect("Size of FeatureTable binary body exceeds maximum size of 4GiB!"),
            batch_table_byte_size
                .try_into()
                .expect("Size of BatchTable header exceeds maximum size of 4GiB!"),
            batch_table_body_byte_size
                .try_into()
                .expect("Size of BatchTable binary body exceeds maximum size of 4GiB!"),
        );

        bincode::serialize_into(&mut self.writer, &pnts_header)
            .context("Error while serializing .pnts header")?;
        self.writer
            .write(feature_table_blob.as_slice())
            .context("Error while writing FeatureTable header")?;
        self.write_feature_table_body()?;
        self.writer
            .write(batch_table_blob.as_slice())
            .context("Error while writing BatchTable header")?;
        // TODO Write BatchTable binary body. For now, it doesn't exist, so we don't have to write anything

        self.requires_flush = false;

        Ok(())
    }

    fn create_feature_table(&self) -> FeatureTableHeader {
        let num_points = self.cached_points.len();
        let cumulative_attribute_offsets = self
            .default_layout
            .attributes()
            .scan(0, |state, attribute| {
                let ret = *state;
                *state +=
                    (attribute.size() as usize * num_points).align_to(PNTS_SEMANTICS_MAX_ALIGNMENT);
                Some(ret)
            })
            .collect::<Vec<_>>();

        let mut point_semantics = self
            .default_layout
            .attributes()
            .enumerate()
            .map(|(idx, attribute)| -> (String, FeatureTableValue) {
                let semantic_name = pnts_semantics_name_from_point_attribute(&attribute.into())
                    .expect("Invalid point semantic");
                (
                    semantic_name,
                    FeatureTableValue::DataReference(FeatureTableDataReference {
                        byte_offset: cumulative_attribute_offsets[idx],
                        component_type: None,
                    }),
                )
            })
            .collect::<HashMap<_, _>>();

        // Create global semantics. Only POINTS_LENGTH is mandatory
        point_semantics.insert(
            "POINTS_LENGTH".into(),
            FeatureTableValue::SingleValue(json!(num_points)),
        );

        point_semantics
    }

    fn create_batch_table(&self) -> BatchTableHeader {
        Default::default()
    }

    /// Calculate the length in bytes of the FeatureTable binary body. This is based on the default PointLayout
    /// and the number of cached points. For simplicities sake, we store all attributes with the same memory
    /// alignment (PNTS_SEMANTICS_MAX_ALIGNMENT), which makes the calculation of total size easier
    fn calc_feature_table_body_length(&self) -> usize {
        let num_points = self.cached_points.len();
        self.default_layout
            .attributes()
            .map(|attribute| {
                (num_points * attribute.size() as usize).align_to(PNTS_SEMANTICS_MAX_ALIGNMENT)
            })
            .sum()
    }

    fn write_feature_table_body(&mut self) -> Result<()> {
        let num_points = self.cached_points.len();
        for attribute in self.default_layout.attributes() {
            let attribute_data = self
                .cached_points
                .get_raw_attribute_range_ref(0..num_points, &attribute.into());
            self.writer
                .write_all(attribute_data)
                .context("Error while writing attribute data")?;

            let blob_byte_size = attribute.size() as usize * self.cached_points.len();
            let num_padding_bytes =
                blob_byte_size.align_to(PNTS_SEMANTICS_MAX_ALIGNMENT) - blob_byte_size;
            if num_padding_bytes != 0 {
                let padding_bytes = vec![0; num_padding_bytes];
                self.writer
                    .write_all(padding_bytes.as_slice())
                    .context("Error while writing padding bytes")?;
            }
        }
        Ok(())
    }
}

impl<W: Write> PointWriter for PntsWriter<W> {
    fn write(&mut self, points: &dyn PointBuffer) -> Result<()> {
        if points.point_layout() != &self.expected_layout {
            panic!("PointLayout of buffer does not match the PointLayout that this PntsReader was constructed with! Make sure that you only pass PointBuffers with the same layout as the one you used to create this PntsWriter!");
        }
        if points.point_layout() == self.cached_points.point_layout() {
            self.cached_points.push(points);
        } else {
            // Have to convert data
            // TODO Depending on the memory layout of `points`, there might be faster ways to push the data than
            // using the generic functions from the `PointBuffer` trait. Revise this method once we have a good API
            // for pushing into a buffer with a different PointLayout!
            let base_point_index = self.cached_points.len();
            self.cached_points
                .resize(self.cached_points.len() + points.len());
            for (attribute_name, maybe_converter) in self.attribute_converters.iter() {
                if let Some(attr) = points.point_layout().get_attribute_by_name(attribute_name) {
                    let attribute_def: PointAttributeDefinition = attr.into();
                    let mut buf = vec![0; attribute_def.size() as usize];
                    let dst_attribute_size = self
                        .cached_points
                        .point_layout()
                        .get_attribute_by_name(attribute_name)
                        .unwrap()
                        .size() as usize;
                    let mut converted_buf = vec![0; dst_attribute_size];
                    for point_index in 0..points.len() {
                        points.get_raw_attribute(point_index, &attribute_def, buf.as_mut_slice());
                        if let Some(conversion_fn) = maybe_converter {
                            unsafe {
                                conversion_fn(buf.as_slice(), converted_buf.as_mut_slice());
                            }
                            self.cached_points.set_raw_attribute(
                                base_point_index + point_index,
                                &attribute_def,
                                converted_buf.as_slice(),
                            );
                        } else {
                            self.cached_points.set_raw_attribute(
                                base_point_index + point_index,
                                &attribute_def,
                                buf.as_slice(),
                            )
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if !self.requires_flush {
            return Ok(());
        }
        self.write_cached_points()
    }

    fn get_default_point_layout(&self) -> &PointLayout {
        &self.default_layout
    }
}

impl<W: Write> Drop for PntsWriter<W> {
    fn drop(&mut self) {
        self.flush().expect("Error while flushing PntsWriter")
    }
}

#[cfg(test)]
mod tests {
    use std::io::SeekFrom;

    use crate::{base::PointReader, tiles3d::PntsReader};

    use super::*;
    use pasture_core::{layout::PointType, nalgebra::Vector3};
    use pasture_derive::PointType;

    #[derive(Debug, PointType, Copy, Clone, PartialEq)]
    #[repr(C, packed)]
    struct PntsDefaultPoint {
        #[pasture(BUILTIN_POSITION_3D)]
        position: Vector3<f32>,
        #[pasture(BUILTIN_COLOR_RGB)]
        color: Vector3<u8>,
        #[pasture(attribute = "Normal")]
        normal: Vector3<f32>,
    }

    #[test]
    fn test_write_pnts_default_layout() -> Result<()> {
        let mut cursor = Cursor::new(Vec::<u8>::new());

        let test_data = vec![
            PntsDefaultPoint {
                position: Vector3::new(1.0, 2.0, 3.0),
                color: Vector3::new(10, 20, 30),
                normal: Vector3::new(0.1, 0.2, 0.3),
            },
            PntsDefaultPoint {
                position: Vector3::new(2.0, 4.0, 6.0),
                color: Vector3::new(20, 40, 60),
                normal: Vector3::new(0.2, 0.4, 0.6),
            },
        ];
        let mut test_point_buffer = PerAttributeVecPointStorage::new(PntsDefaultPoint::layout());
        test_point_buffer.push_points(test_data.as_slice());

        {
            let mut writer =
                PntsWriter::from_write_and_layout(&mut cursor, PntsDefaultPoint::layout());

            writer
                .write(&test_point_buffer)
                .context("Error while writing points to PntsWriter")?;
        }

        cursor.seek(SeekFrom::Start(0))?;

        // Read back in, data read should equal data written
        {
            let mut reader =
                PntsReader::from_read(&mut cursor).context("Error while creating PntsReader")?;
            let read_points = reader
                .read(test_point_buffer.len())
                .context("Error while reading points from PntsReader")?;

            assert_eq!(read_points.point_layout(), test_point_buffer.point_layout());
            assert_eq!(read_points.len(), test_point_buffer.len());

            for point_idx in 0..test_point_buffer.len() {
                let expected_point = test_data[point_idx];
                let actual_point = read_points.get_point::<PntsDefaultPoint>(point_idx);
                assert_eq!(expected_point, actual_point);
            }
        }

        Ok(())
    }
}
