//! Contratos canônicos v1. Artefato versionado, sem tabela e sem domínio.

mod catalog;
mod hash;
mod surface;
mod validate;

pub use catalog::{
    contract_dir, load_schema, schema_path, CatalogError, CONTRACTS, SCHEMA_VERSION,
};
pub use hash::{verify_manifest, HashError};
pub use surface::{
    assert_compatible, extract_surface, load_published_surface, published_from_disk, SurfaceError,
};
pub use validate::{validate_instance, ValidationError};

ap_kernel::declare_module!("contracts");

pub fn verify_release() -> Result<(), ReleaseError> {
    verify_manifest(contract_dir())?;
    let published = load_published_surface()?;
    for name in CONTRACTS {
        let schema = catalog::load_schema(name)?;
        let current = extract_surface(&schema)?;
        let Some(expected) = published.0.get(*name) else {
            return Err(ReleaseError::Surface(SurfaceError::MissingContract(name)));
        };
        assert_compatible(expected, &current)?;
        catalog::assert_envelope(&schema)?;
        for fixture in catalog::list_fixtures(name, true)? {
            validate_instance(&schema, &fixture).map_err(|source| ReleaseError::ValidFixture {
                contract: name,
                source,
            })?;
        }
        let invalid = catalog::list_named_invalid(name)?;
        if invalid.is_empty() {
            return Err(ReleaseError::MissingInvalidFixture(name));
        }
        for (label, fixture) in invalid {
            if validate_instance(&schema, &fixture).is_ok() {
                return Err(ReleaseError::InvalidFixtureAccepted {
                    contract: name,
                    label,
                });
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
pub enum ReleaseError {
    Hash(HashError),
    Surface(SurfaceError),
    Schema(catalog::CatalogError),
    ValidFixture {
        contract: &'static str,
        source: ValidationError,
    },
    InvalidFixtureAccepted {
        contract: &'static str,
        label: String,
    },
    MissingInvalidFixture(&'static str),
}

impl From<HashError> for ReleaseError {
    fn from(value: HashError) -> Self {
        Self::Hash(value)
    }
}

impl From<SurfaceError> for ReleaseError {
    fn from(value: SurfaceError) -> Self {
        Self::Surface(value)
    }
}

impl From<catalog::CatalogError> for ReleaseError {
    fn from(value: catalog::CatalogError) -> Self {
        Self::Schema(value)
    }
}

impl std::fmt::Display for ReleaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hash(error) => write!(formatter, "{error}"),
            Self::Surface(error) => write!(formatter, "{error}"),
            Self::Schema(error) => write!(formatter, "{error}"),
            Self::ValidFixture { contract, source } => {
                write!(
                    formatter,
                    "fixture válida rejeitada em {contract}: {source}"
                )
            }
            Self::InvalidFixtureAccepted { contract, label } => {
                write!(formatter, "fixture inválida aceita em {contract}: {label}")
            }
            Self::MissingInvalidFixture(contract) => {
                write!(formatter, "fixture inválida ausente em {contract}")
            }
        }
    }
}
