//! Local File System functions.

use std::fs;

use crate::execution::run::HandlerSpec;

/// Load tasks from JS files in directory.
/// Return list of filenames and task specs.
pub(crate) fn load_tasks_from_dir(load_dir: std::path::PathBuf) -> Vec<(String, HandlerSpec)> {
    let mut result = vec![];

    match fs::read_dir(load_dir) {
        Err(e) => {
            log::error!("Can't load functions from disk: {}", e);
        }
        Ok(listing) => {
            for file in listing {
                match file {
                    Ok(entry) => {
                        if entry.path().is_file() {
                            match fs::read_to_string(entry.path()) {
                                Err(e) => log::error!("Can't read file: {}", e),
                                Ok(content) => {
                                    result.push((
                                        String::from(entry.path().to_str().unwrap_or("UNKNOWN")),
                                        HandlerSpec {
                                            handler_id: 0,
                                            code: content,
                                        },
                                    ));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Can't load file: {}", e);
                    }
                }
            }
        }
    }

    result
}

/// Load files in directory.
/// Return list of filenames and contents.
pub(crate) fn load_files_from_dir(
    load_dir: std::path::PathBuf,
) -> Result<Vec<(String, String)>, std::io::Error> {
    let mut result = vec![];

    for entry in fs::read_dir(load_dir)? {
        let path = entry?.path();
        if path.is_file() {
            let content = fs::read_to_string(&path)?;
            let path = String::from(path.to_str().unwrap_or("UNKNOWN"));
            result.push((path, String::from(content)));
        }
    }

    Ok(result)
}
