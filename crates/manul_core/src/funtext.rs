pub fn build_logo(submodules: Vec<(&str, &str)>) -> (String, String) {
    let package_version: &str = env!("CARGO_PKG_VERSION");
    let package_author: &str = env!("CARGO_PKG_AUTHORS");

    let info_line = format!("               v{} ({})", package_version, package_author);
    let header_text: String = [
        " ^---^  ███╗   ███╗ █████╗ ███╗   ██╗██╗   ██╗██╗",
        "|  `ω´| ████╗ ████║██╔══██╗████╗  ██║██║   ██║██║",
        "|     | ██╔████╔██║███████║██╔██╗ ██║██║   ██║██║",
        "|     | ██║╚██╔╝██║██╔══██║██║╚██╗██║██║   ██║██║",
        "|     | ██║ ╚═╝ ██║██║  ██║██║ ╚████║╚██████╔╝███████╗",
        "u-----u ╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝ ╚══════╝",
        &info_line,
    ]
    .join("\n");

    let details_text: String = submodules
        .iter()
        .map(|(name, ver)| format!("{} v{}", name, ver))
        .collect::<Vec<String>>()
        .join(", ");
    let details_text_wrapped = textwrap::wrap(&details_text, 64);
    let details_text = details_text_wrapped.join("\n");

    let logo = [header_text.clone(), details_text].join("\n");

    (logo, header_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_logo_empty_submodules() {
        let (logo, header) = build_logo(vec![]);

        assert_eq!(logo, format!("{}\n", header));
    }

    #[test]
    fn test_build_logo_with_submodules() {
        let submodules = vec![("core", "1.0.0"), ("logger", "2.1.0")];
        let (logo, header) = build_logo(submodules);

        assert!(logo.starts_with(&header));

        let details = logo.strip_prefix(&format!("{}\n", header)).unwrap();
        assert_eq!(details, "core v1.0.0, logger v2.1.0");
    }
}
