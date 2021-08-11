use pasture_core::layout::PointAttributeDataType;
use serde::{Deserialize, Serialize};
use static_assertions::{assert_eq_size, const_assert, const_assert_eq};

#[repr(C, packed(1))]
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ExtraBytesRecordRaw {
    _reserved: [u8; 2],
    data_type: u8,
    options: u8,
    name: [i8; 32],
    _unused: [u8; 4],
    no_data: [u8; 8],
    _deprecated_1: [u8; 16],
    min: [u8; 8],
    _deprecated_2: [u8; 16],
    max: [u8; 8],
    _deprecated_3: [u8; 16],
    scale: f64,
    _deprecated_4: [u8; 16],
    offset: f64,
    _deprecated_5: [u8; 16],
    description: [i8; 32],
}

assert_eq_size!(ExtraBytesRecordRaw, [u8; 192]);

/// Describes the meaning of one extra byte record in an LAS/LAZ file. This describes a single value
/// in any of the non-vector [PointAttributeDataType]s. There can be multiple [ExtraBytesRecord]s describing
/// multiple attributes that are encoded in the extra bytes. For more details, see the LAS specification.
pub struct ExtraBytesRecord {
    data_type: PointAttributeDataType,
    offset_to_first_extra_byte: usize,
    no_data_value: Option<[u8; 8]>,
    min_value: Option<[u8; 8]>,
    max_value: Option<[u8; 8]>,
    scale: Option<f64>,
    offset: Option<f64>,
    name: String,
    description: String,
}

impl ExtraBytesRecord {
    /// Returns the corresponding [PointAttributeDataType] for this [ExtraBytesRecord]
    pub fn data_type(&self) -> PointAttributeDataType {
        self.data_type
    }

    /// Returns the NO_DATA value for this [ExtraBytesRecord] as an [f64] value. The NO_DATA value is an optional value indicating a specific
    /// value that represents the absence of data for a point. Per the LAS specification, NO_DATA values are upcast to the largest
    /// primitive type for either unsigned integers, signed integers, or floating-point values. So if the data type of this
    /// [ExtraBytesRecord] is [PointAttributeDataType::F32], the NO_DATA value would be an [f64] value.
    /// # Panics
    /// If the data type of this [ExtraBytesRecord] is not a floating-point data type
    pub fn no_data_value_f64(&self) -> Option<f64> {
        self.no_data_value.as_ref().map(|v| {
            match self.data_type {
                PointAttributeDataType::F32 | PointAttributeDataType::F64 => {
                    f64::from_le_bytes(*v)
                },
                _ => panic!("It is invalid to call no_data_value_f64 if the data type of this ExtraBytesRecord is not a floating-point datatype (i.e. F32 or F64)!"),
            }
        })
    }

    /// Returns the NO_DATA value for this [ExtraBytesRecord] as an [i64] value. The NO_DATA value is an optional value indicating a specific
    /// value that represents the absence of data for a point. Per the LAS specification, NO_DATA values are upcast to the largest
    /// primitive type for either unsigned integers, signed integers, or floating-point values. So if the data type of this
    /// [ExtraBytesRecord] is [PointAttributeDataType::I8], the NO_DATA value would be an [i64] value.
    /// # Panics
    /// If the data type of this [ExtraBytesRecord] is not a signed integer data type
    pub fn no_data_value_i64(&self) -> Option<i64> {
        self.no_data_value.as_ref().map(|v| {
            match self.data_type {
                PointAttributeDataType::I8 | PointAttributeDataType::I16 | PointAttributeDataType::I32 | PointAttributeDataType::I64 => {
                    i64::from_le_bytes(*v)
                },
                _ => panic!("It is invalid to call no_data_value_i64 if the data type of this ExtraBytesRecord is not a signed integer datatype (i.e. I8, I16, I32, or I64)!"),
            }
        })
    }

    /// Returns the NO_DATA value for this [ExtraBytesRecord] as a [u64] value. The NO_DATA value is an optional value indicating a specific
    /// value that represents the absence of data for a point. Per the LAS specification, NO_DATA values are upcast to the largest
    /// primitive type for either unsigned integers, signed integers, or floating-point values. So if the data type of this
    /// [ExtraBytesRecord] is [PointAttributeDataType::U8], the NO_DATA value would be a [u64] value.
    /// # Panics
    /// If the data type of this [ExtraBytesRecord] is not a unsigned integer data type
    pub fn no_data_value_u64(&self) -> Option<u64> {
        self.no_data_value.as_ref().map(|v| {
            match self.data_type {
                PointAttributeDataType::U8 | PointAttributeDataType::U16 | PointAttributeDataType::U32 | PointAttributeDataType::U64 => {
                    u64::from_le_bytes(*v)
                },
                _ => panic!("It is invalid to call no_data_value_u64 if the data type of this ExtraBytesRecord is not an unsigned integer datatype (i.e. U8, U16, U32, or U64)!"),
            }
        })
    }

    /// Returns the minimum value for this [ExtraBytesRecord] as an [f64] value. Just as for the NO_DATA value, the minimum
    /// value is upcast to the largest primitive type for either unsigned integers, signed integers, or floating-point values.
    /// # Panics
    /// If the data type of this [ExtraBytesRecord] is not a floating-point type
    pub fn min_value_f64(&self) -> Option<f64> {
        self.min_value.as_ref().map(|v| {
            match self.data_type {
                PointAttributeDataType::F32 | PointAttributeDataType::F64 => {
                    f64::from_le_bytes(*v)
                },
                _ => panic!("It is invalid to call min_value_f64 if the data type of this ExtraBytesRecord is not a floating-point datatype (i.e. F32 or F64)!"),
            }
        })
    }

    /// Returns the minimum value for this [ExtraBytesRecord] as an [i64] value. Just as for the NO_DATA value, the minimum
    /// value is upcast to the largest primitive type for either unsigned integers, signed integers, or floating-point values.
    /// # Panics
    /// If the data type of this [ExtraBytesRecord] is not a signed integer type
    pub fn min_value_i64(&self) -> Option<i64> {
        self.min_value.as_ref().map(|v| {
            match self.data_type {
                PointAttributeDataType::I8 | PointAttributeDataType::I16 | PointAttributeDataType::I32 | PointAttributeDataType::I64 => {
                    i64::from_le_bytes(*v)
                },
                _ => panic!("It is invalid to call min_value_i64 if the data type of this ExtraBytesRecord is not a signed integer datatype (i.e. I8, I16, I32, I64)!"),
            }
        })
    }

    /// Returns the minimum value for this [ExtraBytesRecord] as a [u64] value. Just as for the NO_DATA value, the minimum
    /// value is upcast to the largest primitive type for either unsigned integers, signed integers, or floating-point values.
    /// # Panics
    /// If the data type of this [ExtraBytesRecord] is not an unsigned integer type
    pub fn min_value_u64(&self) -> Option<u64> {
        self.min_value.as_ref().map(|v| {
            match self.data_type {
                PointAttributeDataType::U8 | PointAttributeDataType::U16 | PointAttributeDataType::U32 | PointAttributeDataType::U64 => {
                    u64::from_le_bytes(*v)
                },
                _ => panic!("It is invalid to call min_value_i64 if the data type of this ExtraBytesRecord is not an unsigned integer datatype (i.e. U8, U16, U32, U64)!"),
            }
        })
    }
}
