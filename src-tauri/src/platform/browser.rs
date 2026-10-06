//! The installed browser a website sign-in opens.
//!
//! Some providers' sign-in, Google's in particular, refuses to run inside an
//! app's embedded window. For those, Quota opens a browser the person already
//! has, Chrome first, then Edge, Chromium and Brave, in a profile of Quota's
//! own. This module only finds the program; it launches nothing. Where each
//! system keeps it is the one unavoidable branch, and it stays here.

use std::path::{Path, PathBuf};

/// The systems whose browser locations differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum System {
    Windows,
    MacOs,
    Linux,
}

impl System {
    /// The system this build runs on.
    pub(crate) const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// A browser Quota can open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Browser {
    /// What the person calls it.
    pub(crate) name: &'static str,
    /// The program to run.
    pub(crate) program: PathBuf,
}

/// The browsers Quota opens, in the order it prefers them.
const NAMES: [&str; 4] = ["Chrome", "Edge", "Chromium", "Brave"];

/// The places each browser is looked for, in order, as (browser, path).
fn candidates(
    system: System,
    variable: &dyn Fn(&str) -> Option<String>,
) -> Vec<(&'static str, PathBuf)> {
    let mut found = Vec::new();
    match system {
        System::Windows => {
            let roots = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"];
            let installs: [(&str, &str); 4] = [
                ("Chrome", r"Google\Chrome\Application\chrome.exe"),
                ("Edge", r"Microsoft\Edge\Application\msedge.exe"),
                ("Chromium", r"Chromium\Application\chrome.exe"),
                (
                    "Brave",
                    r"BraveSoftware\Brave-Browser\Application\brave.exe",
                ),
            ];
            for (name, relative) in installs {
                for root in roots {
                    if let Some(base) = variable(root).filter(|base| !base.trim().is_empty()) {
                        // A Windows path, built as one so the list reads the
                        // same when its tests run on another system.
                        let base = base.trim_end_matches('\\');
                        found.push((name, PathBuf::from(format!("{base}\\{relative}"))));
                    }
                }
            }
        }
        System::MacOs => {
            let apps: [(&str, &str); 4] = [
                ("Chrome", "Google Chrome.app/Contents/MacOS/Google Chrome"),
                ("Edge", "Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
                ("Chromium", "Chromium.app/Contents/MacOS/Chromium"),
                ("Brave", "Brave Browser.app/Contents/MacOS/Brave Browser"),
            ];
            let home = variable("HOME").filter(|home| !home.trim().is_empty());
            for (name, relative) in apps {
                found.push((name, Path::new("/Applications").join(relative)));
                if let Some(home) = &home {
                    found.push((name, Path::new(home).join("Applications").join(relative)));
                }
            }
        }
        System::Linux => {
            let programs: [(&str, &str); 7] = [
                ("Chrome", "google-chrome"),
                ("Chrome", "google-chrome-stable"),
                ("Edge", "microsoft-edge"),
                ("Edge", "microsoft-edge-stable"),
                ("Chromium", "chromium"),
                ("Chromium", "chromium-browser"),
                ("Brave", "brave-browser"),
            ];
            let path = variable("PATH").unwrap_or_default();
            let folders: Vec<&str> = path
                .split(':')
                .filter(|folder| !folder.is_empty())
                .collect();
            for (name, program) in programs {
                for folder in &folders {
                    found.push((name, Path::new(folder).join(program)));
                }
            }
        }
    }
    // Keep the preference order of NAMES across every location.
    found.sort_by_key(|(name, _)| NAMES.iter().position(|known| known == name));
    found
}

/// The first browser found on `system`, given how variables and files are read.
pub(crate) fn find_on(
    system: System,
    variable: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<Browser> {
    candidates(system, variable)
        .into_iter()
        .find(|(_, program)| exists(program))
        .map(|(name, program)| Browser { name, program })
}

/// The first browser installed on this computer.
pub(crate) fn find() -> Option<Browser> {
    find_on(
        System::current(),
        &|name| std::env::var(name).ok(),
        &|path| path.is_file(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variables<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    fn present<'a>(paths: &'a [&'a str]) -> impl Fn(&Path) -> bool + 'a {
        move |path| paths.iter().any(|known| Path::new(known) == path)
    }

    #[test]
    fn windows_prefers_chrome_then_edge_in_the_usual_folders() {
        let vars = variables(&[
            ("ProgramFiles", r"C:\Program Files"),
            ("ProgramFiles(x86)", r"C:\Program Files (x86)"),
            ("LOCALAPPDATA", r"C:\Users\a\AppData\Local"),
        ]);
        let edge = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe";
        let chrome = r"C:\Users\a\AppData\Local\Google\Chrome\Application\chrome.exe";
        let both = [edge, chrome];
        let found = find_on(System::Windows, &vars, &present(&both)).expect("a browser");
        assert_eq!(found.name, "Chrome");
        assert_eq!(found.program, Path::new(chrome));
        let found = find_on(System::Windows, &vars, &present(&[edge])).expect("a browser");
        assert_eq!(found.name, "Edge");
        assert_eq!(find_on(System::Windows, &vars, &present(&[])), None);
    }

    #[test]
    fn windows_skips_a_folder_variable_that_is_not_set() {
        let vars = variables(&[("ProgramFiles", "")]);
        assert!(candidates(System::Windows, &vars).is_empty());
    }

    #[test]
    fn macos_looks_in_applications_and_the_home_applications() {
        let vars = variables(&[("HOME", "/Users/a")]);
        let edge = "/Users/a/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge";
        let found = find_on(System::MacOs, &vars, &present(&[edge])).expect("a browser");
        assert_eq!((found.name, found.program), ("Edge", PathBuf::from(edge)));
        let chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        let found = find_on(System::MacOs, &vars, &present(&[edge, chrome])).expect("a browser");
        assert_eq!(found.name, "Chrome");
    }

    #[test]
    fn linux_searches_the_path_for_each_browser_in_order() {
        let vars = variables(&[("PATH", "/usr/local/bin:/usr/bin::/snap/bin")]);
        let chromium = "/snap/bin/chromium";
        let edge = "/usr/bin/microsoft-edge-stable";
        let found = find_on(System::Linux, &vars, &present(&[chromium, edge])).expect("a browser");
        assert_eq!((found.name, found.program), ("Edge", PathBuf::from(edge)));
        let found = find_on(System::Linux, &vars, &present(&[chromium])).expect("a browser");
        assert_eq!(found.name, "Chromium");
        assert_eq!(
            find_on(System::Linux, &variables(&[]), &present(&[chromium])),
            None
        );
    }
}
