use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub struct Skill {
    pub name: String,
    pub description: String,
    pub location: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SkillHead {
    name: String,
    description: String,
}

pub fn find_skills(path: &PathBuf) -> Vec<Skill> {
    let mut skills = vec![];
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
            skills.push(Skill {
                name: yaml.name,
                description: yaml.description,
                location: entry.path().display().to_string(),
            });
        }
    }
    skills
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

    // 基本行为：解析出 name / description / location
    #[test]
    fn find_parses_name_description_and_location() {
        let root = tempdir().unwrap();
        write_skill(
            &root.path().to_path_buf(),
            "greeter",
            "name: greeter\ndescription: says hi\n",
            "body",
        );
        let skills = find_skills(&root.path().join("skills").to_path_buf());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "greeter");
        assert_eq!(skills[0].description, "says hi");
        assert_eq!(
            skills[0].location,
            root.path()
                .join("skills")
                .join("greeter")
                .display()
                .to_string()
        );
    }

    // 多个 skill 都应找到（read_dir 顺序无保证，排序后再比）
    #[test]
    fn find_includes_all_skills() {
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
        let mut skills = find_skills(&root.path().join("skills").to_path_buf());
        skills.sort_by(|x, y| x.name.cmp(&y.name));
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    // 目录下不存在 skills/ 子目录时（read_dir 失败），返回空 vec 不 panic
    #[test]
    fn find_returns_empty_when_no_skills_dir() {
        let root = tempdir().unwrap();
        assert!(find_skills(&root.path().to_path_buf()).is_empty());
    }

    // 跳过非目录条目（比如 skills/ 下直接放了个文件）
    #[test]
    fn find_skips_non_directory_entries() {
        let root = tempdir().unwrap();
        write_skill(
            &root.path().to_path_buf(),
            "good",
            "name: good\ndescription: g\n",
            "",
        );
        std::fs::write(root.path().join("skills").join("README.md"), "x").unwrap();
        let skills = find_skills(&root.path().join("skills").to_path_buf());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "good");
    }

    // 跳过 SKILL.md 不以 --- 开头（无 frontmatter）的目录
    #[test]
    fn find_skips_skill_without_frontmatter() {
        let root = tempdir().unwrap();
        write_skill(
            &root.path().to_path_buf(),
            "good",
            "name: good\ndescription: g\n",
            "",
        );
        let plain = root.path().join("skills").join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(plain.join("SKILL.md"), "just markdown").unwrap();
        let skills = find_skills(&root.path().join("skills").to_path_buf());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "good");
    }
}
