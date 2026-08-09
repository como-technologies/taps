use std::fs;
use std::path::Path;

use llm_wiki::git;

pub fn setup_wiki(dir: &Path, name: &str) -> std::path::PathBuf {
    let config_path = dir.join("state").join("config.toml");
    let wiki_path = dir.join(name);

    llm_wiki::registry::create(&wiki_path, name, None, false, true, &config_path, None).unwrap();

    let content_root = wiki_path.join("content");
    fs::create_dir_all(content_root.join("concepts")).unwrap();
    fs::write(
        content_root.join("concepts/moe.md"),
        "---\ntitle: \"MoE\"\ntype: concept\nstatus: active\ntags: [ml]\nread_when: [testing]\n---\n\nMixture of Experts.\n",
    )
    .unwrap();
    fs::write(
        content_root.join("concepts/transformer.md"),
        "---\ntitle: \"Transformer\"\ntype: concept\nstatus: active\nread_when: [testing]\n---\n\nAttention is all you need. See [[concepts/moe]].\n",
    )
    .unwrap();
    git::commit(&wiki_path, "add pages").unwrap();

    config_path
}
