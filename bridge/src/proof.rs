use ark_serialize::CanonicalDeserialize;
use bitvm::chunker::disprove_execution::RawProof;

// TODO: replace with actual implementation
pub fn get_proof() -> RawProof {
    // RawProof::default()
    let serialized_data = "687a30a694bb4fb69f2286196ad0d811e702488557c92a923c19499e9c1b3f0105f749dae2cd41b2d9d3998421aa8e86965b1911add198435d50a08892c7cd01a47c01cbf65ccc3b4fc6695671734c3b631a374fbf616e58bcb0a3bd59a9030d7d17433d53adae2232f9ac3caa5c67053d7a728714c81272a8a51507d5c43906010000000000000043a510e31de87bdcda497dfb3ea3e8db414a10e7d4802fc5dddd26e18d2b3a279c3815c2ec66950b63e60c86dc9a2a658e0224d55ea45efe1f633be052dc7d867aff76a9e983210318f1b808aacbbba1dc04b6ac4e6845fa0cc887aeacaf5a068ab9aeaf8142740612ff2f3377ce7bfa7433936aaa23e3f3749691afaa06301fd03f043c097556e7efdf6862007edf3eb868c736d917896c014c54754f65182ae0c198157f92e667b6572ba60e6a52d58cb70dbeb3791206e928ea5e65c6199d25780cedb51796a8a43e40e192d1b23d0cfaf2ddd03e4ade7c327dbc427999244bf4b47b560cf65d672c86ef448eb5061870d3f617bd3658ad6917d0d32d9296020000000000000008f167c3f26c93dbfb91f3077b66bc0092473a15ef21c30f43d3aa96776f352a33622830e9cfcb48bdf8d3145aa0cf364bd19bbabfb3c73e44f56794ee65dc8a";
    let bytes = hex::decode(serialized_data).unwrap();
    RawProof::deserialize_compressed(&*bytes).unwrap()
}
