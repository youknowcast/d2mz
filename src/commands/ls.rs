use anyhow::{Result, bail};

use crate::backend::Backends;
use crate::cli::LsArgs;

pub async fn run(_backends: &mut Backends, _args: LsArgs) -> Result<()> {
    bail!("ls is not implemented yet")
}
