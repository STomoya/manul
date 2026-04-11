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
