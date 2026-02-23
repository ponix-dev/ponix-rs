use crate::domain::DomainResult;

/// Trait for compiling CEL expressions into serialized checked AST bytes.
///
/// Implementations parse + type-check the expression against the standard
/// CEL environment (with `input` variable and custom functions), then serialize
/// the resulting checked AST to protobuf bytes for efficient runtime execution.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
pub trait CelExpressionCompiler: Send + Sync {
    fn compile(&self, expression: &str) -> DomainResult<Vec<u8>>;
}
