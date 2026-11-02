use install_core::Product;

pub fn png(product: Product) -> &'static [u8] {
    match product {
        Product::Acbr => include_bytes!("../../../assets/acbrinstall.png"),
        Product::Pascal => include_bytes!("../../../assets/pascalinstall-256.png"),
    }
}

pub fn window_icon(product: Product) -> eframe::egui::IconData {
    eframe::icon_data::from_png_bytes(png(product)).expect("ícone incorporado inválido")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_products_have_distinct_embedded_icons() {
        let acbr = window_icon(Product::Acbr);
        let pascal = window_icon(Product::Pascal);
        assert_eq!((acbr.width, acbr.height), (256, 256));
        assert_eq!((pascal.width, pascal.height), (256, 256));
        assert_eq!(pascal.rgba.len(), 256 * 256 * 4);
        assert_ne!(acbr.rgba, pascal.rgba);
    }
}
