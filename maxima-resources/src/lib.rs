use std::{io, path::PathBuf};

pub fn maxima_windows_rc(internal_name: &str, display_name: &str) -> io::Result<()> {
    if !cfg!(target_os = "windows") {
        return Ok(());
    }

    let assets_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");

    let license = env!("CARGO_PKG_LICENSE");
    let repository = env!("CARGO_PKG_REPOSITORY");

    let mut res = winres::WindowsResource::new();
    res.set_icon(assets_path.join("logo.ico").to_str().unwrap())
        .set(
            "Comments",
            &format!("Maxima Game Launcher - {}", repository),
        )
        .set("CompanyName", "Armchair Developers")
        .set("FileDescription", display_name)
        .set("InternalName", internal_name)
        .set("LegalTrademarks", license)
        .set("ProductName", display_name);
    res.compile()?;

    Ok(())
}
