use std::io::Cursor;

use base64::Engine;
use image::{ImageDecoder, ImageEncoder};

use super::xml::{direct_child_tag, extract_direct_child_tag_ranges, get_attribute_ref};
use super::{blip_effects, number, percentage, rgb, PictureContext};

pub(super) fn required(blip: &str, source: &str) -> bool {
    source.starts_with("data:image/tiff;")
        || source.starts_with("data:image/tif;")
        || extract_direct_child_tag_ranges(blip).iter().any(|child| {
            matches!(
                child.tag_name.as_str(),
                "a:hsl" | "a:tint" | "a:alphaCeiling" | "a:clrChange"
            )
        })
}

fn decode(source: &str) -> Option<image::RgbaImage> {
    let (header, payload) = source.split_once(',')?;
    if !header.starts_with("data:image/")
        || !header.ends_with(";base64")
        || payload.len() > 96 * 1024 * 1024
    {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .ok()?;
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > 16 * 1024 * 1024 {
        return None;
    }
    let orientation = decoder.orientation().ok()?;
    let mut decoded = image::DynamicImage::from_decoder(decoder).ok()?;
    decoded.apply_orientation(orientation);
    Some(decoded.into_rgba8())
}

fn byte(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn grayscale(pixel: &[u8; 4]) -> u8 {
    ((13927 * u32::from(pixel[0]) + 46885 * u32::from(pixel[1]) + 4725 * u32::from(pixel[2])) >> 16)
        as u8
}

fn rgba(xml: &str, context: &impl PictureContext) -> Option<[u8; 4]> {
    let channels = rgb(xml, context)?;
    let alpha = context.resolve_color(xml)?.opacity.unwrap_or(1.0);
    Some([
        byte(channels[0] * 255.0),
        byte(channels[1] * 255.0),
        byte(channels[2] * 255.0),
        byte(alpha * 255.0),
    ])
}

fn hls(pixel: &[u8; 4]) -> [i32; 3] {
    let [r, g, b] = [
        i32::from(pixel[0]),
        i32::from(pixel[1]),
        i32::from(pixel[2]),
    ];
    let minimum = r.min(g).min(b);
    let maximum = r.max(g).max(b);
    let delta = maximum - minimum;
    let sum = maximum + minimum;
    let lightness = (sum * 240 + 255) / 510;
    if delta == 0 {
        return [160, lightness, 0];
    }
    let divisor = if sum <= 256 { sum } else { 510 - sum };
    let saturation = (delta * 240 + divisor / 2) / divisor;
    let red_delta = ((maximum - r) * 40 + delta / 2) / delta;
    let green_delta = ((maximum - g) * 40 + delta / 2) / delta;
    let blue_delta = ((maximum - b) * 40 + delta / 2) / delta;
    let hue = if maximum == r {
        blue_delta - green_delta
    } else if maximum == g {
        red_delta - blue_delta + 80
    } else {
        green_delta - red_delta + 160
    };
    [hue.rem_euclid(240), lightness, saturation]
}

fn from_hls(pixel: &mut [u8; 4], [hue, lightness, saturation]: [i32; 3]) {
    if saturation == 0 {
        pixel[..3].fill(((lightness as u32 * 0x880077) >> 23) as u8);
        return;
    }
    let maximum = if lightness <= 120 {
        ((saturation + 240) * lightness + 120) / 240
    } else {
        lightness + saturation - (lightness * saturation + 120) / 240
    };
    let minimum = 2 * lightness - maximum;
    for (channel, hue) in pixel.iter_mut().take(3).zip([hue + 80, hue, hue - 80]) {
        let hue = hue.rem_euclid(240);
        let value = if hue < 40 {
            minimum + ((maximum - minimum) * hue + 20) / 40
        } else if hue < 120 {
            maximum
        } else if hue <= 160 {
            minimum + ((maximum - minimum) * (160 - hue) + 20) / 40
        } else {
            minimum
        };
        *channel = ((value * 255 + 120) / 240) as u8;
    }
}

pub(super) fn render(blip: &str, source: &str, context: &impl PictureContext) -> Option<String> {
    let mut image = decode(source)?;
    for child in blip_effects(blip) {
        let xml = &blip[child.start..child.end];
        match child.tag_name.as_str() {
            "a:extLst" => {}
            "a:grayscl" | "a:biLevel" => {
                let threshold = percentage(xml, "thresh", 0.5)?.clamp(0.0, 1.0) * 255.0;
                for pixel in image.pixels_mut() {
                    let gray = grayscale(&pixel.0);
                    let gray = if child.tag_name == "a:biLevel" {
                        if f64::from(gray) >= threshold {
                            255
                        } else {
                            0
                        }
                    } else {
                        gray
                    };
                    pixel.0[..3].fill(gray);
                }
            }
            "a:alphaCeiling" => {
                for pixel in image.pixels_mut() {
                    if pixel[3] > 0 {
                        pixel[3] = 255;
                    }
                }
            }
            "a:alphaFloor" => {
                for pixel in image.pixels_mut() {
                    if pixel[3] < 255 {
                        pixel[3] = 0;
                    }
                }
            }
            "a:alphaBiLevel" => {
                let threshold = percentage(xml, "thresh", 0.5)?.clamp(0.0, 1.0) * 255.0;
                for pixel in image.pixels_mut() {
                    pixel[3] = if f64::from(pixel[3]) >= threshold {
                        255
                    } else {
                        0
                    };
                }
            }
            "a:alphaModFix" => {
                let amount = percentage(xml, "amt", 1.0)?.max(0.0);
                for pixel in image.pixels_mut() {
                    pixel[3] = byte(f64::from(pixel[3]) * amount);
                }
            }
            "a:alphaRepl" => {
                let alpha = byte(percentage(xml, "a", 1.0)?.clamp(0.0, 1.0) * 255.0);
                for pixel in image.pixels_mut() {
                    pixel[3] = alpha;
                }
            }
            "a:alphaInv" => {
                let color = if extract_direct_child_tag_ranges(xml).is_empty() {
                    [0, 0, 0, 255]
                } else {
                    rgba(xml, context)?
                };
                for pixel in image.pixels_mut() {
                    pixel[3] = 255 - pixel[3];
                    pixel.0[..3].copy_from_slice(&color[..3]);
                }
            }
            "a:clrRepl" => {
                let color = rgba(xml, context)?;
                for pixel in image.pixels_mut() {
                    pixel.0[..3].copy_from_slice(&color[..3]);
                }
            }
            "a:clrChange" => {
                let from = rgba(direct_child_tag(xml, "a:clrFrom")?, context)?;
                let to = rgba(direct_child_tag(xml, "a:clrTo")?, context)?;
                let alpha = !get_attribute_ref(xml, "useA")
                    .is_some_and(|value| matches!(value, "0" | "false" | "off"));
                for pixel in image.pixels_mut() {
                    if alpha && pixel.0 == from {
                        pixel.0 = to;
                    } else if !alpha && pixel.0[..3] == from[..3] {
                        pixel.0[..3].copy_from_slice(&to[..3]);
                    }
                }
            }
            "a:duotone" => {
                let children = extract_direct_child_tag_ranges(xml);
                let [dark, light] = children.as_slice() else {
                    return None;
                };
                let dark = rgba(&xml[dark.start..dark.end], context)?;
                let light = rgba(&xml[light.start..light.end], context)?;
                for pixel in image.pixels_mut() {
                    let gray = f64::from(grayscale(&pixel.0)) / 255.0;
                    for index in 0..3 {
                        pixel[index] = byte(
                            f64::from(dark[index])
                                + (f64::from(light[index]) - f64::from(dark[index])) * gray,
                        );
                    }
                }
            }
            "a:lum" => {
                let brightness = (percentage(xml, "bright", 0.0)?.clamp(-1.0, 1.0) * 0.5) as f32;
                let contrast = percentage(xml, "contrast", 0.0)?.clamp(-1.0, 1.0);
                let slope = (if contrast <= 0.0 {
                    1.0 + contrast
                } else {
                    1.0 / (1.0 - contrast.min(0.99999))
                }) as f32;
                let intercept = slope * (brightness - 0.5) + brightness + 0.5;
                for pixel in image.pixels_mut() {
                    for channel in &mut pixel.0[..3] {
                        *channel = byte(
                            f64::from(slope * (f32::from(*channel) / 255.0) + intercept) * 255.0,
                        );
                    }
                }
            }
            "a:tint" => {
                let target = (number(xml, "hue", 0.0)? / 21_600_000.0) as f32;
                if !(0.0..=1.0).contains(&target) {
                    return None;
                }
                let amount = percentage(xml, "amt", 0.0)?.clamp(-1.0, 1.0) as f32;
                for pixel in image.pixels_mut() {
                    let mut channels = hls(&pixel.0);
                    let mut current = channels[0] as f32 / 240.0;
                    if (target - current).abs() > 0.5 {
                        current += if target > current { 1.0 } else { -1.0 };
                    }
                    let mut result = (target - current).mul_add(amount, current);
                    if result > 1.0 {
                        result -= 1.0;
                    }
                    if result < 0.0 {
                        result += 1.0;
                    }
                    channels[0] = (result * 240.0) as i32;
                    from_hls(&mut pixel.0, channels);
                }
            }
            "a:hsl" => {
                let rotation = (number(xml, "hue", 0.0)? / 21_600_000.0) as f32 * 240.0;
                if !rotation.is_finite() {
                    return None;
                }
                let rotation = (rotation.trunc() % 240.0) as i32;
                let lightness = percentage(xml, "sat", 0.0)?.max(0.0) as f32;
                let saturation = percentage(xml, "lum", 0.0)?.max(0.0) as f32;
                for pixel in image.pixels_mut() {
                    let mut channels = hls(&pixel.0);
                    channels[0] = (channels[0] + rotation).rem_euclid(240);
                    channels[1] = (lightness * channels[1] as f32).min(240.0) as i32;
                    channels[2] = (saturation * channels[2] as f32).min(240.0) as i32;
                    from_hls(&mut pixel.0, channels);
                }
            }
            _ => return None,
        }
    }
    encode(&image)
}

pub(super) fn normalize(source: &str) -> Option<String> {
    encode(&decode(source)?)
}

fn encode(image: &image::RgbaImage) -> Option<String> {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    if png.len() > 72 * 1024 * 1024 {
        return None;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}
