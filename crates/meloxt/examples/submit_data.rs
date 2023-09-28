// Copyright 2023 ZeroDAO

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at

//     http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use log::{error, info};
use meloxt::info_msg::*;
use meloxt::init_logger;
use meloxt::sidercar_metadata_runtime;
use meloxt::{melodot, ClientBuilder};

#[tokio::main]
pub async fn main() {
	init_logger().unwrap();

	if let Err(err) = run().await {
		error!("{}", err);
	}
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
	info!("{} submit data", START_EXAMPLE);
	let client = ClientBuilder::default().build().await?;

	let app_id = 1;
	let bytes_len = 121; // Exceeding the limit
	let (commitments, proofs, data_hash, _) = sidercar_metadata_runtime(bytes_len);

	let submit_data_tx =
		melodot::tx()
			.melo_store()
			.submit_data(app_id, bytes_len, data_hash, commitments, proofs);

	let block_hash = client
		.api
		.tx()
		.sign_and_submit_then_watch_default(&submit_data_tx, &client.signer)
		.await?
		.wait_for_finalized_success()
		.await?
		.block_hash();

	info!("{}: Data submited, block hash: {}", SUCCESS, block_hash);

	info!("{} : Submit data", ALL_SUCCESS);

	Ok(())
}