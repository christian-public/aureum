use aureum::TestId;
use std::path::{Path, PathBuf};
use std::str;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum TestPath {
    Glob(String),
    SpecificFile {
        source_file: PathBuf,
        test_id: TestId,
    },
}

impl str::FromStr for TestPath {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let (path, Some(suffix)) = aureum::split_file_name(Path::new(s)) {
            if path.is_file() {
                Ok(Self::SpecificFile {
                    source_file: path,
                    test_id: TestId::from(suffix.as_str()),
                })
            } else {
                Err("Invalid path to config file")
            }
        } else {
            Ok(Self::Glob(s.to_owned()))
        }
    }
}
