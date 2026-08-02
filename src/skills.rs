use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct SkillHead {
    name: String,
    description: String,
}

pub fn list_skills(dir: &PathBuf) -> String {
    let path = dir.join("skills");
    let mut skills = String::new();
    if let Ok(dir) = std::fs::read_dir(path) {
        for entry in dir.flatten() {
            if !entry.path().is_dir() {
                continue;
            }

            let skill_md = entry.path().join("SKILL.md");
            let content = std::fs::read_to_string(skill_md).unwrap();
            if !content.starts_with("---") {
                continue;
            }

            let parts: Vec<&str> = content.split("---").collect();
            let yaml: SkillHead = serde_yaml::from_str(parts[1].trim()).unwrap();
            skills.push_str(&format!("- **{}**: {}\n", yaml.name, yaml.description));
        }
    }
    skills
}

pub fn load_skill(dir: &PathBuf, name: &str) -> String {
    let path = dir.join("skills").join(name).join("SKILL.md");
    std::fs::read_to_string(path).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // 在 dir/skills/<name>/SKILL.md 写入内容
    fn write_skill(root: &PathBuf, name: &str, frontmatter: &str, body: &str) {
        let skill_dir = root.join("skills").join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let content = format!("---\n{frontmatter}---\n{body}");
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    // 基本格式：每个 skill 渲染成 `- **name**: description`
    #[test]
    fn list_formats_skills_as_bullets() {
        let root = tempdir().unwrap();
        write_skill(
            &root.path().to_path_buf(),
            "greeter",
            "name: greeter\ndescription: says hi\n",
            "body",
        );
        let out = list_skills(&root.path().to_path_buf());
        assert_eq!(out, "- **greeter**: says hi\n");
    }

    // 多个 skill 都应列出
    #[test]
    fn list_includes_all_skills() {
        let root = tempdir().unwrap();
        write_skill(
            &root.path().to_path_buf(),
            "a",
            "name: a\ndescription: aa\n",
            "",
        );
        write_skill(
            &root.path().to_path_buf(),
            "b",
            "name: b\ndescription: bb\n",
            "",
        );
        let out = list_skills(&root.path().to_path_buf());
        assert!(out.contains("**a**"), "got: {out}");
        assert!(out.contains("**b**"), "got: {out}");
    }

    // 目录下不存在 skills/ 子目录时（read_dir 失败），返回空串不 panic
    #[test]
    fn list_returns_empty_when_no_skills_dir() {
        let root = tempdir().unwrap();
        let out = list_skills(&root.path().to_path_buf());
        assert_eq!(out, "");
    }

    // 跳过非目录条目（比如 skills/ 下直接放了个文件）
    #[test]
    fn list_skips_non_directory_entries() {
        let root = tempdir().unwrap();
        // 一个正常 skill
        write_skill(
            &root.path().to_path_buf(),
            "good",
            "name: good\ndescription: g\n",
            "",
        );
        // 一个平铺文件（不是目录）
        std::fs::write(root.path().join("skills").join("README.md"), "x").unwrap();
        let out = list_skills(&root.path().to_path_buf());
        assert!(out.contains("**good**"), "got: {out}");
        assert!(!out.contains("README"), "got: {out}");
    }

    // 跳过 SKILL.md 不以 --- 开头（无 frontmatter）的目录
    #[test]
    fn list_skips_skill_without_frontmatter() {
        let root = tempdir().unwrap();
        write_skill(
            &root.path().to_path_buf(),
            "good",
            "name: good\ndescription: g\n",
            "",
        );
        // 没有 frontmatter 的 skill
        let plain = root.path().join("skills").join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(plain.join("SKILL.md"), "just markdown").unwrap();
        let out = list_skills(&root.path().to_path_buf());
        assert!(out.contains("**good**"), "got: {out}");
        assert!(!out.contains("plain"), "got: {out}");
    }

    // load_skill 读取完整 SKILL.md 内容（含 frontmatter 与正文）
    #[test]
    fn load_returns_full_content() {
        let root = tempdir().unwrap();
        write_skill(
            &root.path().to_path_buf(),
            "s",
            "name: s\ndescription: d\n",
            "instructions here",
        );
        let out = load_skill(&root.path().to_path_buf(), "s");
        assert!(out.contains("instructions here"), "got: {out}");
        assert!(out.contains("name: s"), "got: {out}");
    }
}
