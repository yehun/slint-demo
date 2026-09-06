use slint::Color;


pub fn string_to_color(color: String) -> Color {
    color
        .parse::<css_color_parser2::Color>()
        .map(|c| Color::from_argb_u8((c.a * 255.) as u8, c.r, c.g, c.b))
        .unwrap_or(Color::default())
}