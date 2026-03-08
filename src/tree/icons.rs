use std::path::Path;

/// Get icon for a file based on its name and extension.
pub fn get_file_icon(filename: &str) -> char {
    match filename {
        // Rust
        "Cargo.toml" | "Cargo.lock" => '\u{e7a8}', //

        // Git
        ".gitignore" | ".gitmodules" | ".gitattributes" => '\u{f1d3}', //

        // Build files
        "Makefile" | "makefile" => '\u{e779}', //
        "CMakeLists.txt" => '\u{e779}',        //

        // Config
        ".editorconfig" => '\u{e615}', //

        // Documentation
        "README" | "README.md" => '\u{f48a}',  //
        "LICENSE" | "CHANGELOG" => '\u{f15c}', //
        "CHANGELOG.md" => '\u{f48a}',          //

        // Docker
        "Dockerfile" => '\u{e7b0}',                                 //
        "docker-compose.yml" | "docker-compose.yaml" => '\u{e7b0}', //

        _ => {
            if let Some(extension) = Path::new(filename).extension() {
                if let Some(ext) = extension.to_str() {
                    match ext.to_lowercase().as_str() {
                        // Systems / compiled
                        "rs" => '\u{e7a8}',                                 //
                        "c" | "h" => '\u{e61e}',                            //
                        "cpp" | "cxx" | "cc" | "hpp" | "hxx" => '\u{e61d}', //
                        "go" => '\u{e724}',                                 //
                        "java" | "class" | "jar" => '\u{e738}',             //

                        // Scripted / interpreted
                        "py" | "pyc" | "pyo" | "pyw" => '\u{e73c}', //
                        "rb" => '\u{e739}',                         //
                        "sh" | "bash" | "zsh" | "fish" => '\u{f489}', //
                        "lua" => '\u{e620}',                        //

                        // Web
                        "js" | "jsx" | "mjs" => '\u{e74e}', //
                        "ts" | "tsx" => '\u{e628}',         //
                        "html" | "htm" => '\u{e736}',       //
                        "css" | "scss" | "sass" | "less" => '\u{e749}', //
                        "vue" => '\u{fd42}',                //
                        "svelte" => '\u{e697}',             //

                        // Config / data
                        "json" => '\u{e60b}',                 //
                        "yaml" | "yml" => '\u{f481}',         //
                        "toml" => '\u{e615}',                 //
                        "ini" | "conf" | "cfg" => '\u{e615}', //
                        "xml" => '\u{f05c}',                  //
                        "env" => '\u{f462}',                  //

                        // Docs
                        "md" | "markdown" => '\u{f48a}', //
                        "txt" | "text" => '\u{f15c}',    //
                        "pdf" => '\u{f1c1}',             //
                        "csv" => '\u{f1c3}',             //

                        // Images
                        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" => '\u{f1c5}', //

                        // Archives
                        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => '\u{f1c6}', //

                        // Lock files
                        "lock" => '\u{f023}', //

                        _ => '\u{f15b}', //
                    }
                } else {
                    '\u{f15b}' //
                }
            } else {
                '\u{f15b}' //
            }
        }
    }
}

/// Get icon for a directory based on its expanded state.
pub fn get_directory_icon(expanded: bool) -> char {
    if expanded {
        '\u{f115}' //  Open folder
    } else {
        '\u{f114}' //  Closed folder
    }
}
