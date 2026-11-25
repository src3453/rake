pub fn write_sb3(path: &Path, project: &Value, assets: &[(String, Vec<u8>)]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let file =
        File::create(path).map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive
        .start_file("project.json", options)
        .map_err(|error| error.to_string())?;
    archive
        .write_all(
            serde_json::to_string(project)
                .map_err(|error| error.to_string())?
                .as_bytes(),
        )
        .map_err(|error| error.to_string())?;
    for (name, contents) in assets {
        archive
            .start_file(name, options)
            .map_err(|error| error.to_string())?;
        archive
            .write_all(contents)
            .map_err(|error| error.to_string())?;
    }
    archive.finish().map_err(|error| error.to_string())?;
    Ok(())
}

pub fn write_project_json(path: &Path, project: &Value) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let contents = serde_json::to_string_pretty(project).map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| format!("cannot write {}: {error}", path.display()))
}
