use std::{path::PathBuf};

fn main() {
    // node 1 
    let proto_file = ["./src/node_1/proto/request.proto"];
    let proto_out_dir = ["./src/node_1"];

    for file in &proto_file {
        if !PathBuf::from(file).exists() {
            panic!("Proto file {} does not exist", file);
        }
    }
    
    tonic_build::configure()
        .out_dir(&proto_out_dir[0])
        .compile(&proto_file, &["./src/node_1"])
        .expect("Failed to compile protobuf files");

    // node 2
    let proto_file = ["./src/node_2/proto/request.proto"];
    let proto_out_dir = ["./src/node_2"];

    for file in &proto_file {
        if !PathBuf::from(file).exists() {
            panic!("Proto file {} does not exist", file);
        }
    }
    
    tonic_build::configure()
        .out_dir(&proto_out_dir[0])
        .compile(&proto_file, &["./src/node_2"])
        .expect("Failed to compile protobuf files");
}