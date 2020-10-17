use actix_web_static_files::NpmBuild;
use std::path::Path;
use std::fs;

fn visit_dirs(dir: &Path) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path)?;
            } else {
                println!("cargo:rerun-if-changed={}", path.to_string_lossy());
            }
        }
    }
    Ok(())
}

fn main() {
    visit_dirs(Path::new("site")).unwrap();
    NpmBuild::new("./site")
    .install().unwrap()
    .run("build").unwrap()
    .target("./site/dist");
}