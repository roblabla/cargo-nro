extern crate linkle;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

use std::env::{self, VarError};
use std::fmt;
use std::process::{Command, Stdio};
use std::path::{Path, PathBuf};
use std::fs::File;
use linkle::format::nxo::NxoFile;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct Artifact {
    package_id: String,
    target: Target,
    profile: ArtifactProfile,
    features: Vec<String>,
    filenames: Vec<String>,
    fresh: bool
}


#[derive(Deserialize, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum TargetKind {
    Lib,
    Bin,
    Test,
    Bench,
    Example,
    CustomBuild,
}

impl fmt::Debug for TargetKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::TargetKind::*;
        match *self {
            Lib => "lib".fmt(f),
            Bin => "bin".fmt(f),
            Example => "example".fmt(f),
            Test => "test".fmt(f),
            CustomBuild => "custom-build".fmt(f),
            Bench => "bench".fmt(f),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Target {
    /// Is this a `--bin bin`, `--lib`, `--example ex`?
    /// Serialized as a list of strings for historical reasons.
    kind: Vec<TargetKind>,
    /// Corresponds to `--crate-type` compiler attribute.
    /// See https://doc.rust-lang.org/reference/linkage.html
    crate_types: Vec<String>,
    name: String,
    src_path: PathBuf,
    #[serde(default = "default_edition")]
    edition: String,
    #[serde(rename = "required-features", default)]
    required_features: Option<Vec<String>>,
}

fn default_edition() -> String {
    String::from("2015")
}

#[derive(Debug, Deserialize)]
pub struct ArtifactProfile {
    opt_level: String,
    debuginfo: Option<u32>,
    debug_assertions: bool,
    overflow_checks: bool,
    test: bool,
}

#[derive(Debug, Deserialize)]
struct FromCompiler {
    package_id: String,
    target: Target,
    message: Value,
}

#[derive(Debug, Deserialize)]
struct BuildScript {
    package_id: String,
    linked_libs: Vec<String>,
    linked_paths: Vec<String>,
    cfgs: Vec<String>,
    env: Vec<(String, String)>
}

#[serde(tag = "reason")]
#[derive(Debug, Deserialize)]
enum Message {
    #[serde(rename = "compiler-artifact")]
    Artifact(Artifact),
    #[serde(rename = "compiler-message")]
    Message(FromCompiler),
    #[serde(rename = "build-script-executed")]
    BuildScript(BuildScript),
}

/*impl Message {
    fn render(&self) -> String {
        match self {
        }
    }
}*/

fn find_project_root(path: &Path) -> Option<&Path> {
    for parent in path.ancestors() {
        if parent.join("Cargo.toml").is_file() {
            return Some(parent);
        }
    }
    None
}

fn main() {
    let rust_target_path = match env::var("RUST_TARGET_PATH") {
        Err(VarError::NotPresent) => {
            // TODO: Handle workspace
            find_project_root(&env::current_dir().unwrap()).unwrap().into()
        },
        s => PathBuf::from(s.unwrap()),
    };

    let command = Command::new("xargo")
        .args(&["build", "--target=aarch64-roblabla-switch", "--message-format=json"])
        .stdout(Stdio::piped())
        .env("RUST_TARGET_PATH", rust_target_path.as_os_str())
        .spawn().unwrap();

    let iter = serde_json::Deserializer::from_reader(command.stdout.unwrap()).into_iter::<Message>();
    for message in iter {
        match message {
            Ok(Message::Artifact(ref artifact)) if artifact.target.kind[0] == TargetKind::Bin => {
                // Find the artifact's source. This is not going to be pretty.
                let src = artifact.target.src_path.clone();
                let mut romfs = None;
                let root = find_project_root(&src).unwrap();
                if root.join("res").is_dir() {
                    romfs = Some(root.join("res").to_string_lossy().into_owned());
                }
                let mut new_name = PathBuf::from(artifact.filenames[0].clone());
                assert!(new_name.set_extension("nro"));
                NxoFile::from_elf(&artifact.filenames[0]).unwrap().write_nro(&mut File::create(new_name.clone()).unwrap(), romfs.as_ref().map(|v| v.as_ref())).unwrap();
                println!("Built {} (using {:?} as romfs)", new_name.to_string_lossy(), romfs);
            },
            Ok(Message::Artifact(_artifact)) => {
                //println!("{:#?}", artifact);
            },
            Ok(Message::Message(msg)) => {
                match msg.message {
                    //Value::String(s) => println!("{}", s),
                    Value::Object(v) => {
                        println!("{}", v.get("rendered").unwrap().as_str().unwrap());
                        println!("{:?}", v.get("children").unwrap());
                    },
                    v => panic!("WTF: {:?}", v)
                }
            },
            Ok(_) => (),
            Err(ref err) if err.is_data() => {
                println!("{:?}", err);
            },
            Err(err) => {
                panic!("{:?}", err);
            }
        }
    }
}
