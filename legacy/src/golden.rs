#![allow(dead_code)]
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

/// Assert the golden file matches.
#[macro_export]
macro_rules! assert_golden {
    ($test_name:expr, $actual:expr) => {{
        let g = $crate::_new_goldie!($test_name);
        if let Err(err) = g.assert($actual) {
            ::std::panic!("{}", err);
        }
    }};
    ($actual:expr) => {{
        let g = $crate::_new_goldie!();
        if let Err(err) = g.assert($actual) {
            ::std::panic!("{}", err);
        }
    }};
}

/// Assert the golden file matches the debug output.
#[macro_export]
macro_rules! assert_golden_debug {
    ($actual:expr) => {{
        let g = $crate::_new_goldie!();
        if let Err(err) = g.assert_debug($actual) {
            ::std::panic!("{}", err);
        }
    }};
}

/// Assert the templated golden file matches.
#[macro_export]
macro_rules! assert_golden_template {
    ($ctx:expr, $actual:expr) => {{
        let g = $crate::_new_goldie!();
        if let Err(err) = g.assert_template($ctx, $actual) {
            ::std::panic!("{}", err);
        }
    }};
}

/// Assert the JSON golden file matches.
#[macro_export]
macro_rules! assert_golden_json {
    ($test_name:expr, $actual:expr) => {{
        let g = $crate::_new_goldie!($test_name);
        if let Err(err) = g.assert_json($actual) {
            ::std::panic!("{}", err);
        }
    }};
    ($actual:expr) => {{
        let g = $crate::_new_goldie!();
        if let Err(err) = g.assert_json($actual) {
            ::std::panic!("{}", err);
        }
    }};
}

/// Constructs a new goldie instance.
/// Not public API.
#[doc(hidden)]
#[macro_export]
macro_rules! _new_goldie {
    () => {{
        let source_file =
            $crate::golden::cargo_workspace_dir(env!("CARGO_MANIFEST_DIR")).join(file!());
        let function_path = $crate::_function_path!();
        $crate::golden::Goldie::new(source_file, function_path, None)
    }};
    ($test_name:expr) => {{
        let source_file =
            $crate::golden::cargo_workspace_dir(env!("CARGO_MANIFEST_DIR")).join(file!());
        let function_path = $crate::_function_path!();
        $crate::golden::Goldie::new(source_file, function_path, Some(&$test_name))
    }};
}

/// Returns the fully qualified path to the current item.
///
/// Goldie uses this to get the name of the test function.
///
/// Not public API.
#[doc(hidden)]
#[macro_export]
macro_rules! _function_path {
    () => {{
        fn f() {}
        fn type_name_of_val<T>(_: T) -> &'static str {
            ::std::any::type_name::<T>()
        }
        let mut name = type_name_of_val(f).strip_suffix("::f").unwrap_or("");
        while let Some(rest) = name.strip_suffix("::{{closure}}") {
            name = rest;
        }
        name
    }};
}

#[derive(Debug)]
pub struct Goldie {
    /// The path to the golden file.
    golden_file: PathBuf,
    /// Whether to update the golden file if it doesn't match.
    update: bool,
}

impl Goldie {
    /// Construct a new golden file tester.
    ///
    /// Where
    /// - `source_file` is path to the source file that the test resides in.
    /// - `function_path` is the full path to the function. e.g.
    ///   `crate::module::tests::function_name`.
    pub fn new(
        source_file: impl AsRef<Path>,
        function_path: impl AsRef<str>,
        test_name: Option<&str>,
    ) -> Self {
        Self::new_impl(source_file.as_ref(), function_path.as_ref(), test_name)
    }

    fn new_impl(source_file: &Path, function_path: &str, test_name: Option<&str>) -> Self {
        let (_, name) = function_path.rsplit_once("::").unwrap();

        let golden_file = {
            let mut p = source_file.parent().unwrap().to_owned();
            p.push("golden");
            p.push(name);
            if let Some(test_name) = test_name {
                p.push(test_name);
            };
            p.set_extension("golden");
            p
        };

        let update = matches!(
            env::var("GOLDEN_UPDATE").ok().as_deref(),
            Some("1" | "true")
        );

        Self {
            golden_file,
            update,
        }
    }

    #[track_caller]
    pub fn assert(&self, actual: impl AsRef<str>) -> Result<()> {
        if self.update {
            let dir = self.golden_file.parent().unwrap();
            fs::create_dir_all(dir)?;
            fs::write(&self.golden_file, actual.as_ref())?;
        } else {
            let expected = fs::read_to_string(&self.golden_file)
                .with_context(|| self.error("failed to read golden file"))?;
            pretty_assertions::assert_eq!(
                actual.as_ref(),
                expected,
                "\n\ngolden file `{}` does not match",
                self.golden_file
                    .strip_prefix(env::current_dir()?)?
                    .display(),
            );
        }
        Ok(())
    }

    #[track_caller]
    pub fn assert_debug(&self, actual: impl std::fmt::Debug) -> Result<()> {
        self.assert(format!("{actual:#?}"))
    }

    #[track_caller]
    pub fn assert_template(&self, ctx: impl Serialize, actual: impl AsRef<str>) -> Result<()> {
        static ENGINE: Lazy<upon::Engine> = Lazy::new(|| {
            upon::Engine::with_syntax(upon::SyntaxBuilder::new().expr("{{", "}}").build())
        });

        let contents = fs::read_to_string(&self.golden_file)
            .with_context(|| self.error("failed to read golden file"))?;
        let expected = ENGINE
            .compile(&contents)
            .with_context(|| self.error("failed to compile golden file template"))?
            .render(&ENGINE, &ctx)
            .to_string()
            .with_context(|| self.error("failed to render golden file template"))?;

        pretty_assertions::assert_eq!(
            actual.as_ref(),
            expected,
            "\n\ngolden file `{}` does not match",
            self.golden_file
                .strip_prefix(env::current_dir()?)?
                .display(),
        );

        Ok(())
    }

    #[track_caller]
    pub fn assert_json(&self, actual: impl Serialize) -> Result<()> {
        if self.update {
            let dir = self.golden_file.parent().unwrap();
            fs::create_dir_all(dir)?;
            fs::write(
                &self.golden_file,
                serde_json::to_string_pretty(&actual).unwrap(),
            )?;
        } else {
            let contents = fs::read_to_string(&self.golden_file)
                .with_context(|| self.error("failed to read golden file"))?;
            let expected: serde_json::Value =
                serde_json::from_str(&contents).with_context(|| self.error("bad JSON"))?;
            let actual: serde_json::Value = serde_json::to_value(&actual)?;

            pretty_assertions::assert_eq!(
                actual,
                expected,
                "\n\ngolden file `{}` does not match",
                self.golden_file
                    .strip_prefix(env::current_dir()?)?
                    .display(),
            );
        }

        Ok(())
    }

    fn error(&self, msg: &str) -> String {
        use yansi::Color;
        format!(
            "\n\n{}: {}\nrun with {} to regenerate the golden file\n\n",
            Color::Red.paint(msg),
            self.golden_file.display(),
            Color::Blue.paint("GOLDEN_UPDATE=1").bold(),
        )
    }
}

/// Returns the Cargo workspace dir for the given manifest dir.
///
/// Not public API.
#[doc(hidden)]
pub fn cargo_workspace_dir(manifest_dir: &str) -> PathBuf {
    static DIRS: Lazy<Mutex<BTreeMap<String, Arc<Path>>>> =
        Lazy::new(|| Mutex::new(BTreeMap::new()));

    let mut dirs = DIRS.lock().unwrap();

    if let Some(dir) = dirs.get(manifest_dir) {
        return dir.to_path_buf();
    }

    let dir = env::var("CARGO_WORKSPACE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            #[derive(Deserialize)]
            struct Manifest {
                workspace_root: PathBuf,
            }
            let cargo = env::var_os("CARGO");
            let cargo = cargo.as_deref().unwrap_or_else(|| OsStr::new("cargo"));
            let output = process::Command::new(cargo)
                .args(["metadata", "--format-version=1", "--no-deps"])
                .current_dir(manifest_dir)
                .output()
                .unwrap();
            let manifest: Manifest = serde_json::from_slice(&output.stdout).unwrap();
            manifest.workspace_root
        });
    dirs.insert(
        String::from(manifest_dir),
        dir.clone().into_boxed_path().into(),
    );

    dir
}
