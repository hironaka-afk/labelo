use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct LabelConfigString {
    pub name: String,
    pub states: Vec<String>,
    pub optional: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LabelConfigInt {
    pub name: String,
    pub first: i32,
    pub last: i32,
    pub optional: bool,
}

/// LabelConfig which can represent optional labels.
pub(crate) trait LabelConfigOptional {
    fn is_optional(&self) -> bool;
}

impl LabelConfigOptional for LabelConfigString {
    fn is_optional(&self) -> bool { self.optional }
}

impl LabelConfigOptional for LabelConfigInt {
    fn is_optional(&self) -> bool { self.optional }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum LabelConfig {
    /// The bool is determining whether the label is optional.
    S(LabelConfigString),
    /// The bool is determining whether the label is optional.
    I(LabelConfigInt),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LabelConfigs {
    pub label_configs: Vec<LabelConfig>
}

impl Default for LabelConfigs {
    fn default() -> Self {
        let l = LabelConfigString { name: "animal".to_string(), 
        states: vec!["cat".to_string(),"dog".to_string(),"possum".to_string()],
        optional: false };

        let li = LabelConfigInt { name: "size".to_string(), first: 1, last: 10, optional: true };
     
        Self { label_configs: vec![
            LabelConfig::S(l),
            LabelConfig::I(li)] 
        }
    }

}

#[derive(Serialize, Deserialize, Clone)]
pub struct LabelInstance<T> {
    pub name: String,
    pub state: T
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Label {
    S(LabelInstance<String>),
    I(LabelInstance<i32>)
}


#[cfg(test)]
mod tests {
    use super::*;
    use toml;
    use serde_json;
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use dirs::home_dir;

    #[test]
    fn label_config_serialize() {
        let l = LabelConfigString { name: "ALabel".to_string(), 
            states: vec!["One".to_string(),"Two".to_string(),"Three".to_string()],
            optional: false,
        };

        let li = LabelConfigInt { name: "ILabel".to_string(), first: 1, last: 5, optional: true };

        let s = toml::to_string(&l).unwrap();
        println!("{}", s);

        let lc = LabelConfigs { label_configs: vec![
            LabelConfig::S(l.clone()),
            LabelConfig::I(li.clone()),
        ] };
        let s2 = toml::to_string(&lc).unwrap();
        println!("{}", s2);

        let s3 = serde_json::to_string_pretty(&lc).unwrap();
        println!("JSON: {}", s3);
    }

    #[test]
    fn label_serialize() {
        let l = Label::S(LabelInstance::<String> {name: "ALabel".to_string(), state: "AState".to_string()});
        let s = toml::to_string(&l).unwrap();
        println!("{}", s);

        let s2 = serde_json::to_string_pretty(&l).unwrap();
        println!("{}", s2);

    }
    
    #[test]
    fn create_default_configs() {
        let c = LabelConfigs::default();

        let outfile = home_dir().expect("Home dir not found").join(Path::new("labelo_config.toml"));
        let mut f = fs::File::create(outfile).expect("Failed to create config file.");
        let _ = write!(f, "{}", toml::to_string_pretty(&c).unwrap_or("Oops".to_string()));
    }
}