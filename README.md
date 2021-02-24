# pasture

Rust library for point cloud processing

**Under development** 

## Core

- [ ] PointBuffer
    - [x] InterleavedPointBuffer
        - [ ] Figure out ways of accessing the point data in known (but variable) formats, such as the 11 LAS formats
    - [x] PerAttributePointBuffer
        - [ ] Improve `push_attribute` and `push_attribute_range`
    - [ ] Support iterator `collect`
- [ ] Point Views
    - [x] Interleaved view
    - [x] PerAttribute view
    - [ ] Performance checks on views for different PointBuffers
    - [ ] Attribute iterators using macro_rules!
        - [x] v1
        - [ ] Extensive tests
- [ ] PointType
    - [x] Basic `PointType` structure
    - [x] Procedural macro for implementing `PointType` for a type
        - [ ] Check robustness on types that are `#[repr(packed)]` but not `#[repr(C)]`
        - [x] Documentation of the `BUILTIN_...` and `attribute = "..."` syntax
        - [x] Get rid of warnings, clean up code
- [ ] Examples of usage

## I/O

- [ ] LAS/LAZ
    - [ ] Reader
        - [x] Format 0
        - [x] Format 1
        - [x] Format 2
        - [x] Format 3
        - [x] Format 4
        - [x] Format 5
        - [x] Format 6
        - [x] Format 7
        - [x] Format 8
        - [x] Format 9
        - [x] Format 10
        - [x] Attribute conversions (e.g. positions as I32, F32, F64)
            - [ ] Works in principle, but requires many more tests to be robust 
        - [x] SeekToPoint
            - [x] Tests
    - [ ] Writer
        - [x] Migrate `LASWriter` to use `RawLASWriter` and `RawLAZWriter`
            - [x] Implement `RawLAZWriter`
                - [x] Default format
                - [x] Custom format
        - [x] Format 0
        - [x] Format 1
        - [x] Format 2
        - [x] Format 3
        - [x] Format 4
        - [x] Format 5
        - [ ] Format 6
        - [ ] Format 7
        - [ ] Format 8
        - [ ] Format 9
        - [ ] Format 10
        - [ ] Attribute conversions (e.g. positions as I32, F32, F64)
        - [ ] SeekToPoint
            - [ ] Support SeekToPoint in Writer? 
    - [ ] Metadata
        - [x] Basic metadata structure
        - [ ] Support for additional attributes in header
        - [ ] Support for VLRs
            - [ ] Define how VLRs should be represented 
    - [x] Benchmarks
        - [ ] Maybe extend the benchmarks for more point layouts
    - [ ] Examples
- [ ] 3D Tiles
    - [ ] Reader
    - [ ] Writer