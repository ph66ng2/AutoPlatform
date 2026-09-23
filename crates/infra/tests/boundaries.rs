use std::{collections::BTreeSet, fs, path::Path};

const PROCESS_CRATES: &[&str] = &[
    "ap-kernel",
    "ap-infra",
    "ap-api",
    "ap-worker",
    "ap-observability",
    "ap-contracts",
];

#[test]
fn workspace_dependencies_stay_acyclic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("raiz");
    let workspace: toml::Value = read_toml(&root.join("Cargo.toml"));
    let members = workspace["workspace"]["members"]
        .as_array()
        .expect("members");

    let mut packages = Vec::new();
    for member in members {
        let relative = member.as_str().expect("member");
        let manifest_path = root.join(relative).join("Cargo.toml");
        let manifest = read_toml(&manifest_path);
        let name = manifest["package"]["name"]
            .as_str()
            .expect("package.name")
            .to_string();
        let deps = dependency_names(&manifest);
        assert_paths_stay_inside(&root, &manifest_path, &manifest);
        packages.push((name, deps));
    }

    let internal: BTreeSet<String> = packages.iter().map(|(name, _)| name.clone()).collect();
    let domains: Vec<String> = internal
        .iter()
        .filter(|name| !PROCESS_CRATES.contains(&name.as_str()))
        .cloned()
        .collect();
    let mut domain_modules: Vec<&str> = domains
        .iter()
        .map(|name| name.strip_prefix("ap-").expect("prefixo ap-"))
        .collect();
    domain_modules.sort_unstable();
    assert_eq!(domain_modules, ap_kernel::REQUIRED_MODULES);

    for (package, deps) in &packages {
        let internal_deps: Vec<&str> = deps
            .iter()
            .filter(|dep| internal.contains(*dep))
            .map(String::as_str)
            .collect();
        let allowed = allowed_internal(package, &domains);
        for dep in &internal_deps {
            assert!(
                allowed.contains(dep),
                "{package} não pode depender de {dep}"
            );
        }
        if package != "ap-kernel" {
            assert!(
                internal_deps.contains(&"ap-kernel"),
                "{package} precisa depender de ap-kernel"
            );
        }
        if package == "ap-api" || package == "ap-worker" {
            for domain in &domains {
                assert!(
                    internal_deps.contains(&domain.as_str()),
                    "{package} não liga o módulo {domain}"
                );
            }
        }
    }
}

fn allowed_internal<'a>(package: &str, domains: &'a [String]) -> Vec<&'a str> {
    match package {
        "ap-kernel" => Vec::new(),
        "ap-observability" => vec!["ap-kernel"],
        "ap-contracts" => vec!["ap-kernel"],
        "ap-infra" => vec!["ap-kernel", "ap-observability"],
        "ap-audit" => vec!["ap-kernel", "ap-observability"],
        "ap-api" | "ap-worker" => {
            let mut allowed = vec!["ap-kernel", "ap-infra"];
            allowed.extend(domains.iter().map(String::as_str));
            allowed
        }
        other if domains.iter().any(|domain| domain == other) => vec!["ap-kernel"],
        other => panic!("crate sem regra de fronteira: {other}"),
    }
}

fn dependency_names(manifest: &toml::Value) -> Vec<String> {
    let mut names = Vec::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(table) = manifest.get(section).and_then(toml::Value::as_table) else {
            continue;
        };
        for (key, value) in table {
            let package = value
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(key);
            names.push(package.to_string());
        }
    }
    names
}

fn assert_paths_stay_inside(root: &Path, manifest_path: &Path, manifest: &toml::Value) {
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(table) = manifest.get(section).and_then(toml::Value::as_table) else {
            continue;
        };
        for (key, value) in table {
            let Some(path) = value.get("path").and_then(toml::Value::as_str) else {
                continue;
            };
            let resolved = manifest_path
                .parent()
                .expect("dir")
                .join(path)
                .canonicalize()
                .unwrap_or_else(|err| panic!("{key} path {path}: {err}"));
            assert!(
                resolved.starts_with(root),
                "dependência {key} aponta para fora do workspace"
            );
        }
    }
}

fn read_toml(path: &Path) -> toml::Value {
    let text = fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    toml::from_str(&text).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}
