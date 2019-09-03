
use futures::Future;
use hyper::client::connect::{Destination, HttpConnector};
use tower_grpc::Request;
use tower_hyper::{client, util};
use tower_util::MakeService;
use futures::stream::Stream;

use std::sync::Arc;

mod lightclient;
mod address;
mod prover;

pub mod grpc_client {
    include!(concat!(env!("OUT_DIR"), "/cash.z.wallet.sdk.rpc.rs"));
}



pub fn main() {
    let lightclient = Arc::new(lightclient::Client::new());
    lightclient.set_initial_block(500000,
                            "004fada8d4dbc5e80b13522d2c6bd0116113c9b7197f0c6be69bc7a62f2824cd",
                            "01b733e839b5f844287a6a491409a991ec70277f39a50c99163ed378d23a829a0700100001916db36dfb9a0cf26115ed050b264546c0fa23459433c31fd72f63d188202f2400011f5f4e3bd18da479f48d674dbab64454f6995b113fa21c9d8853a9e764fb3e1f01df9d2c233ca60360e3c2bb73caf5839a1be634c8b99aea22d02abda2e747d9100001970d41722c078288101acd0a75612acfb4c434f2a55aab09fb4e812accc2ba7301485150f0deac7774dcd0fe32043bde9ba2b6bbfff787ad074339af68e88ee70101601324f1421e00a43ef57f197faf385ee4cac65aab58048016ecbd94e022973701e1b17f4bd9d1b6ca1107f619ac6d27b53dd3350d5be09b08935923cbed97906c0000000000011f8322ef806eb2430dc4a7a41c1b344bea5be946efc7b4349c1c9edb14ff9d39");

    let mut last_scanned_height = lightclient.last_scanned_height() as u64;
    let mut end_height = last_scanned_height + 1000;

    loop {
        let local_lightclient = lightclient.clone();

        let simple_callback = move |encoded_block: &[u8]| {
            local_lightclient.scan_block(encoded_block);
            
            println!("Block Height: {}, Balance = {}", local_lightclient.last_scanned_height(), local_lightclient.balance());
        };

        read_blocks(last_scanned_height, end_height, simple_callback);

        if end_height < 588000 {
            last_scanned_height = end_height + 1;
            end_height = last_scanned_height + 1000 - 1;
        }
    }    
}

pub fn read_blocks<F : 'static + std::marker::Send>(start_height: u64, end_height: u64, c: F)
    where F : Fn(&[u8]) {
    // Fetch blocks
    let uri: http::Uri = format!("http://127.0.0.1:9067").parse().unwrap();

    let dst = Destination::try_from_uri(uri.clone()).unwrap();
    let connector = util::Connector::new(HttpConnector::new(4));
    let settings = client::Builder::new().http2_only(true).clone();
    let mut make_client = client::Connect::with_builder(connector, settings);

    let say_hello = make_client
        .make_service(dst)
        .map_err(|e| panic!("connect error: {:?}", e))
        .and_then(move |conn| {
            use crate::grpc_client::client::CompactTxStreamer;

            let conn = tower_request_modifier::Builder::new()
                .set_origin(uri)
                .build(conn)
                .unwrap();

            // Wait until the client is ready...
            CompactTxStreamer::new(conn)
                .ready()
                .map_err(|e| eprintln!("streaming error {:?}", e))
        })
        .and_then(move |mut client| {
            use crate::grpc_client::BlockId;
            use crate::grpc_client::BlockRange;
            

            let bs = BlockId{ height: start_height, hash: vec!()};
            let be = BlockId{ height: end_height,   hash: vec!()};

            let br = Request::new(BlockRange{ start: Some(bs), end: Some(be)});
            client
                .get_block_range(br)
                .map_err(|e| {
                    eprintln!("RouteChat request failed; err={:?}", e);
                })
                .and_then(move |response| {
                    let inbound = response.into_inner();
                    inbound.for_each(move |b| {
                        use prost::Message;
                        let mut encoded_buf = vec![];

                        b.encode(&mut encoded_buf).unwrap();
                        c(&encoded_buf);

                        Ok(())
                    })
                    .map_err(|e| eprintln!("gRPC inbound stream error: {:?}", e))                    
                })
        });

    tokio::run(say_hello);
    println!("All done!");
}