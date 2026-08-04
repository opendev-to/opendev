import sys

def replace_block(file_path):
    with open(file_path, "r") as f:
        content = f.read()

    # 1. Replace json unix
    content = content.replace(
        "            std::io::Write::write_all(&mut opts.open(&tmp_json)?, json_content.as_bytes())?;",
        "            let mut file = opts.open(&tmp_json)?;\n            std::io::Write::write_all(&mut file, json_content.as_bytes())?;\n            file.sync_all()?;"
    )

    # 2. Replace json non-unix
    content = content.replace(
"""            std::io::Write::write_all(
                &mut std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&tmp_json)?,
                json_content.as_bytes(),
            )?;""",
"""            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp_json)?;
            std::io::Write::write_all(&mut file, json_content.as_bytes())?;
            file.sync_all()?;"""
    )

    # 3. Replace jsonl unix
    content = content.replace(
        "            std::io::Write::write_all(&mut opts.open(&tmp_jsonl)?, jsonl_content.as_bytes())?;",
        "            let mut file = opts.open(&tmp_jsonl)?;\n            std::io::Write::write_all(&mut file, jsonl_content.as_bytes())?;\n            file.sync_all()?;"
    )

    # 4. Replace jsonl non-unix
    content = content.replace(
"""            std::io::Write::write_all(
                &mut std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&tmp_jsonl)?,
                jsonl_content.as_bytes(),
            )?;""",
"""            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp_jsonl)?;
            std::io::Write::write_all(&mut file, jsonl_content.as_bytes())?;
            file.sync_all()?;"""
    )

    with open(file_path, "w") as f:
        f.write(content)

replace_block("crates/opendev-history/src/session_manager/mod.rs")
