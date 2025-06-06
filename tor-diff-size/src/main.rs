use std::fs;

use arti_client::{TorClient, TorClientConfig};
use sha3::{Digest, Sha3_256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tor_circmgr::DirInfo;

fn compute_consensus_digest(content: &str) -> String {
    let needle = "directory-signature ";
    let stop = content.find(needle).unwrap();
    let signed_part = &content[..stop + needle.len()];
    let mut hash = Sha3_256::new();
    hash.update(signed_part.as_bytes());
    let result = hash.finalize();
    return hex::encode(result).to_uppercase();
}

#[tokio::main]
async fn main() {
    println!("Creating Tor client...");
    let client = TorClient::create_bootstrapped(TorClientConfig::default())
        .await
        .unwrap();
    let circmgr = client.circmgr();
    let circ = circmgr
        .get_or_launch_dir(DirInfo::Directory(
            &client.dirmgr().timely_netdir().unwrap(),
        ))
        .await
        .unwrap();
    println!("Got circuit to a directory cache");

    for arg in std::env::args().skip(1) {
        println!("Attempting {arg}");

        let content = fs::read_to_string(&arg).unwrap();
        let cons_hash = compute_consensus_digest(&content);
        println!("  hash: {cons_hash}");

        let mut stream = circ.clone().begin_dir_stream().await.unwrap();
        println!("  got DIR stream");

        let flavor = if arg.contains("-microdesc") {
            "-microdesc"
        } else {
            ""
        };

        stream
            .write_all(
                format!("GET /tor/status-vote/current/consensus{flavor} HTTP/1.1\r\n").as_bytes(),
            )
            .await
            .unwrap();
        stream
            .write_all(b"Accept-Encoding: zstd,identity\r\n")
            .await
            .unwrap();
        stream
            .write_all(format!("X-Or-Diff-From-Consensus: {cons_hash}\r\n").as_bytes())
            .await
            .unwrap();
        stream.write_all(b"\r\n").await.unwrap();
        stream.flush().await.unwrap();

        let mut result = Vec::new();
        stream.read_to_end(&mut result).await.unwrap();

        println!("  saving result as...");
        let outpath = format!("{arg}.diff");
        fs::write(&outpath, result).unwrap();
        println!("  {outpath}");
    }
}
