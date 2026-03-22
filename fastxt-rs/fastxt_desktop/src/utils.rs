use anyhow::Result;
use druid::widget::{Svg, SvgData};
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode, Version};

/// Build an SVG QR code widget from raw data.
///
/// # Errors
/// Returns an error if the QR code cannot be encoded with the given version and error-correction level.
///
/// # Panics
/// Panics if the rendered SVG string fails to parse (should not happen with well-formed output).
pub fn qrcode_builder<D: AsRef<[u8]>>(
    data: D,
    version: Version,
    eclevel: EcLevel,
    min_dim: (u32, u32),
    dark_color: &str,
    light_color: &str,
) -> Result<Svg> {
    Ok(Svg::new(
        QrCode::with_version(data, version, eclevel)?
            .render()
            .min_dimensions(min_dim.0, min_dim.1)
            .dark_color(svg::Color(dark_color))
            .light_color(svg::Color(light_color))
            .build()
            .parse::<SvgData>()
            .unwrap(),
    ))
}
