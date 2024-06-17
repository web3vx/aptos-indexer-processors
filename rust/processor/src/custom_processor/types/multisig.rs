use serde::{Deserialize, Serialize};

use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, TypeTag},
};

use crate::custom_processor::serde_helper::vec_bytes;

#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum MultisigTransactionPayload {
    EntryFunction(EntryFunction),
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct EntryFunction {
    pub module: ModuleId,
    pub function: Identifier,
    pub ty_args: Vec<TypeTag>,
    #[serde(with = "vec_bytes")]
    pub args: Vec<Vec<u8>>,
}

impl EntryFunction {
    pub fn new(
        module: ModuleId,
        function: Identifier,
        ty_args: Vec<TypeTag>,
        args: Vec<Vec<u8>>,
    ) -> Self {
        EntryFunction {
            module,
            function,
            ty_args,
            args,
        }
    }

    pub fn module(&self) -> &ModuleId {
        &self.module
    }

    pub fn function(&self) -> &IdentStr {
        &self.function
    }

    pub fn ty_args(&self) -> &[TypeTag] {
        &self.ty_args
    }

    pub fn args(&self) -> &[Vec<u8>] {
        &self.args
    }

    pub fn into_inner(self) -> (ModuleId, Identifier, Vec<TypeTag>, Vec<Vec<u8>>) {
        (self.module, self.function, self.ty_args, self.args)
    }

    pub fn as_entry_function_payload(&self) -> EntryFunctionPayload {
        EntryFunctionPayload::new(
            self.module.address,
            self.module.name().to_string(),
            self.function.to_string(),
            self.ty_args.iter().map(|ty| ty.to_string()).collect(),
            self.args.clone(),
        )
    }
}

#[derive(Debug, Clone)]
pub struct EntryFunctionPayload {
    pub account_address: AccountAddress,
    pub module_name: String,
    pub function_name: String,
    pub ty_arg_names: Vec<String>,
    pub args: Vec<Vec<u8>>,
}
impl EntryFunctionPayload {
    pub fn new(
        account_address: AccountAddress,
        module_name: String,
        function_name: String,
        ty_arg_names: Vec<String>,
        args: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            account_address,
            module_name,
            function_name,
            ty_arg_names,
            args,
        }
    }
}
