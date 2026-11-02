use anyhow::{bail, Context, Result};
use std::{io::Read, time::Duration};

#[derive(Clone, Copy)]
pub enum Component {
    Fpc,
    Lazarus,
}

impl Component {
    fn project(self) -> &'static str {
        match self {
            Self::Fpc => "fpc/source",
            Self::Lazarus => "lazarus/lazarus",
        }
    }

    fn release_prefix(self) -> &'static str {
        match self {
            Self::Fpc => "release_",
            Self::Lazarus => "lazarus_",
        }
    }

    fn is_release(self, tag: &str) -> bool {
        tag.strip_prefix(self.release_prefix()).is_some_and(|v| {
            v.starts_with(|c: char| c.is_ascii_digit())
                && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    }

    /// Consulta as tags Git sem precisar instalar o cliente git.
    pub fn releases(self) -> Result<Vec<String>> {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(20))
            .build();
        let project = self.project().replace('/', "%2F");
        let mut tags = Vec::new();
        let mut page = 1;
        loop {
            let url = format!(
                "https://gitlab.com/api/v4/projects/freepascal.org%2F{project}/repository/tags?per_page=100&page={page}"
            );
            let response = agent
                .get(&url)
                .set("User-Agent", "ACBrInstall")
                .call()
                .context("não foi possível consultar as tags no GitLab")?;
            let next = response.header("x-next-page").unwrap_or("").to_string();
            let mut body = String::new();
            response.into_reader().read_to_string(&mut body)?;
            let entries: serde_json::Value = serde_json::from_str(&body)?;
            let entries = entries.as_array().context("lista de tags inválida")?;
            tags.extend(
                entries
                    .iter()
                    .filter_map(|entry| entry["name"].as_str())
                    .filter(|tag| self.is_release(tag))
                    .map(String::from),
            );
            if next.is_empty() {
                break;
            }
            let next: usize = next.parse().context("paginação de tags inválida")?;
            if next <= page {
                bail!("paginação de tags não avançou");
            }
            page = next;
        }
        if tags.is_empty() {
            bail!("nenhuma tag de release encontrada");
        }
        // Compara os componentes numericamente: 4.10 vem antes de 4.8.
        tags.sort_by_cached_key(|tag| {
            let version = tag.trim_start_matches(self.release_prefix());
            let numbers: Vec<u32> = version
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|n| n.parse().ok())
                .collect();
            std::cmp::Reverse((numbers, tag.clone()))
        });
        tags.dedup();
        tags.insert(0, "stable".into());
        Ok(tags)
    }

    /// URLs de arquivos fixados na tag funcionam mesmo sem aliases no fpclazup.
    pub fn source(self, selection: &str) -> String {
        if !self.is_release(selection) {
            return selection.into();
        }
        let project = self.project();
        let name = project.rsplit('/').next().unwrap();
        format!("https://gitlab.com/freepascal.org/{project}/-/archive/{selection}/{name}-{selection}.zip")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_development_tags_and_keeps_release_candidates() {
        assert!(Component::Fpc.is_release("release_3_2_2"));
        assert!(Component::Fpc.is_release("release_3_2_4_rc2"));
        assert!(!Component::Fpc.is_release("pas2js/release_2_2_0"));
        assert!(!Component::Fpc.is_release("3.3.1"));
        assert!(Component::Lazarus.is_release("lazarus_4_8"));
        assert!(Component::Lazarus.is_release("lazarus_5_0_RC1"));
        assert!(!Component::Lazarus.is_release("main_5_99"));
        assert!(!Component::Lazarus.is_release("t-fixes-5"));
    }

    #[test]
    fn installation_uses_selected_tag_without_changing_cli_aliases() {
        assert_eq!(Component::Fpc.source("stable"), "stable");
        assert_eq!(Component::Lazarus.source("trunk"), "trunk");
        assert_eq!(Component::Fpc.source("3.2.2"), "3.2.2");
        assert_eq!(Component::Fpc.source("release_3_2_2"),
            "https://gitlab.com/freepascal.org/fpc/source/-/archive/release_3_2_2/source-release_3_2_2.zip");
        assert_eq!(Component::Lazarus.source("lazarus_4_8"),
            "https://gitlab.com/freepascal.org/lazarus/lazarus/-/archive/lazarus_4_8/lazarus-lazarus_4_8.zip");
    }
}
