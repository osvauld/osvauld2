use super::*;

#[test]
fn mip_chain_reaches_one_pixel_and_averages_in_linear_light() {
    // Left column black, right column white: the 1x1 level is mid-grey in linear light (188),
    // not the sRGB midpoint (128).
    let px = |v: u8| [v, v, v, 255];
    let rgba = [px(0), px(255), px(0), px(255)].concat();
    let t = TextureData::new(2, 2, rgba).unwrap();
    assert_eq!(t.mips.len(), 2);
    assert_eq!(t.mips[1].data(), &[188, 188, 188, 255]);
    assert_eq!(t.payload_bytes(), payload_bytes(2, 2));
    assert_eq!(payload_bytes(5, 3), (15 + 2 + 1) * 4);
    let wide = TextureData::new(2048, 1, vec![9; 2048 * 4]).unwrap();
    assert_eq!(wide.mips.len(), 12);
    assert_eq!(wide.mips[11].data(), &[9, 9, 9, 9]);
}

#[test]
fn sizes_and_data_are_checked() {
    assert_eq!(TextureData::new(0, 1, vec![]).unwrap_err(), TextureError::InvalidSize);
    assert_eq!(
        TextureData::new(2049, 1, vec![0; 2049 * 4]).unwrap_err(),
        TextureError::InvalidSize
    );
    assert_eq!(TextureData::new(2, 2, vec![0; 15]).unwrap_err(), TextureError::InvalidData);
    let a = TextureData::new(1, 1, vec![1, 2, 3, 4]).unwrap();
    let b = TextureData::new(1, 1, vec![1, 2, 3, 4]).unwrap();
    assert_ne!(a.resource_id(), b.resource_id());
}
