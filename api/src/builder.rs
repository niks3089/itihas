use jsonrpsee::RpcModule;
use log::debug;

use crate::error::ApiError;
use crate::spec::{ApiContract, GetTransactionsByAddress};

pub struct RpcApiBuilder;

impl RpcApiBuilder {
    pub fn build(
        contract: Box<dyn ApiContract>,
    ) -> Result<RpcModule<Box<dyn ApiContract>>, ApiError> {
        let mut module = RpcModule::new(contract);

        module.register_async_method("liveness", |_rpc_params, rpc_context| async move {
            debug!("Checking Liveness");
            rpc_context.liveness().await.map_err(Into::into)
        })?;

        module.register_async_method("readiness", |_rpc_params, rpc_context| async move {
            debug!("Checking Readiness");
            rpc_context.readiness().await.map_err(Into::into)
        })?;

        // get_transactions_by_address
        module.register_async_method(
            "get_transactions_by_address",
            |rpc_params, rpc_context| async move {
                let payload = rpc_params.parse::<GetTransactionsByAddress>()?;
                rpc_context
                    .get_transactions_by_address(payload)
                    .await
                    .map_err(Into::into)
            },
        )?;

        module.register_async_method("schema", |_, rpc_context| async move {
            Ok(rpc_context.schema())
        })?;
        module.register_alias("api_schema", "schema")?;
        module.register_alias("apiSchema", "schema")?;

        Ok(module)
    }
}
