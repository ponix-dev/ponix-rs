use crate::cel::create_cel_env;
use crate::cel::CelExpressionCompiler;
use crate::domain::{DomainError, DomainResult};
use cel_core::Env;
use cel_core_proto::AstToProto;
use prost::Message;

/// Compiles CEL expressions into serialized checked AST bytes.
///
/// Uses the shared CEL environment (standard library + `input` variable +
/// `cayenne_lpp_decode` function) to parse and type-check expressions, then
/// serializes the checked AST to protobuf bytes via `cel-core-proto`.
pub struct CelCompiler {
    env: Env,
}

impl CelCompiler {
    pub fn new() -> Self {
        Self {
            env: create_cel_env(),
        }
    }
}

impl Default for CelCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl CelExpressionCompiler for CelCompiler {
    fn compile(&self, expression: &str) -> DomainResult<Vec<u8>> {
        let ast = self.env.compile(expression).map_err(|e| {
            DomainError::PayloadConversionError(format!("CEL compilation error: {e}"))
        })?;

        let checked = ast.to_checked_expr().map_err(|e| {
            DomainError::PayloadConversionError(format!("CEL AST conversion error: {e}"))
        })?;

        Ok(checked.encode_to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_valid_expression() {
        let compiler = CelCompiler::new();
        let result = compiler.compile("true");
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn test_compile_complex_expression() {
        let compiler = CelCompiler::new();
        let result = compiler.compile("cayenne_lpp_decode(input)");
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn test_compile_invalid_expression() {
        let compiler = CelCompiler::new();
        let result = compiler.compile("invalid{[syntax");
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(DomainError::PayloadConversionError(_))
        ));
    }

    #[test]
    fn test_compile_produces_decodable_bytes() {
        use cel_core_proto::CheckedExpr;

        let compiler = CelCompiler::new();
        let bytes = compiler.compile("size(input) > 0").unwrap();
        let decoded = CheckedExpr::decode(bytes.as_slice());
        assert!(decoded.is_ok());
    }
}
