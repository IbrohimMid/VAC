use vil_sdk::prelude::*;

let sink = HttpSinkBuilder::new().port(3080).path("/trigger").build();
let source = HttpSourceBuilder::new()
    .url("http://upstream/stream")
    .format(HttpFormat::SSE)
    .build();

// BUG: route direction is reversed
let (_ir, handles) = vil_workflow! {
    name: "Gateway",
    token: ShmToken,
    instances: [sink, source],
    routes: [
        source.out -> sink.in (LoanWrite),
    ]
};
