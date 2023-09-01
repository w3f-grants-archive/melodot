// Copyright 2023 ZeroDAO
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use futures::StreamExt;
use melo_core_primitives::{traits::Extractor, Encode};
use sc_network::{KademliaKey, NetworkDHTProvider};
use sc_transaction_pool_api::{InPoolTransaction, TransactionPool};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::sync::Arc;

const LOG_TARGET: &str = "tx_pool_listener";

use crate::{NetworkProvider, Sidercar, SidercarMetadata};

fn sidercar_kademlia_key(sidercar: &Sidercar) -> KademliaKey {
	KademliaKey::from(Vec::from(sidercar.id()))
}

#[derive(Clone)]
pub struct TPListenerParams<Client, Network, TP> {
	pub client: Arc<Client>,
	pub network: Arc<Network>,
	pub transaction_pool: Arc<TP>,
}

pub async fn start_tx_pool_listener<Client, Network, TP, B>(
	TPListenerParams { client, network, transaction_pool }: TPListenerParams<Client, Network, TP>,
) where
	Network: NetworkProvider + 'static,
	TP: TransactionPool<Block = B> + 'static,
	B: BlockT + Send + Sync + 'static,
	Client: HeaderBackend<B> + ProvideRuntimeApi<B>,
	Client::Api: Extractor<B>,
{
	tracing::info!(
		target: LOG_TARGET,
		"Starting transaction pool listener.",
	);
	// Obtain the import notification event stream from the transaction pool
	let mut import_notification_stream = transaction_pool.import_notification_stream();

	// Handle the transaction pool import notification event stream
	while let Some(notification) = import_notification_stream.next().await {
		match transaction_pool.ready_transaction(&notification) {
			Some(transaction) => {
				// TODO: Can we avoid decoding the extrinsic here?
				let encoded = transaction.data().encode();
				let at = client.info().best_hash;
				match client.runtime_api().extract(at, &encoded) {
					Ok(res) => match res {
						Some(data) => {
							data.into_iter().for_each(
								|(data_hash, bytes_len, commitments, proofs)| {
									tracing::debug!(
										target: LOG_TARGET,
										"New blob transaction found. Hash: {:?}", data_hash,
									);

									let metadata = SidercarMetadata {
										data_len: bytes_len,
										blobs_hash: data_hash,
										commitments,
										proofs,
									};

									let fetch_value_from_network = |sidercar: &Sidercar| {
										network.get_value(&sidercar_kademlia_key(sidercar));
									};

									match Sidercar::from_local(&metadata.id()) {
										Some(sidercar) => {
											if sidercar.status.is_none() {
												fetch_value_from_network(&sidercar);
											}
										},
										None => {
											let sidercar =
												Sidercar { blobs: None, metadata, status: None };
											sidercar.save_to_local();
											fetch_value_from_network(&sidercar);
										},
									}
								},
							);
						},
						None => {
							tracing::debug!(
								target: LOG_TARGET,
								"Decoding of extrinsic failed. Transaction: {:?}",
								transaction.hash(),
							);
						},
					},
					Err(err) => {
						tracing::debug!(
							target: LOG_TARGET,
							"Failed to extract data from extrinsic. Transaction: {:?}. Error: {:?}",
							transaction.hash(),
							err,
						);
					},
				};
			},
			None => {},
		}
	}
}