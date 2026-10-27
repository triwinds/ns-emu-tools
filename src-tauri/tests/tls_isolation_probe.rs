//! Run through `python src-tauri/tests/tls_isolation.py` from the repository root.
//! Each invocation runs in a separate process with its own certificate environment.

use std::error::Error;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires the local HTTPS fixture supplied by tls_isolation.py"]
async fn tls_isolation_probe() {
    let url = std::env::var("NS_EMU_TLS_TEST_URL").expect("fixture URL");
    let expected = std::env::var("NS_EMU_TLS_TEST_EXPECT").expect("expected outcome");
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10));

    // Control case: reproduce the previous built-in-roots-only configuration.
    if std::env::var_os("NS_EMU_TLS_TEST_DISABLE_NATIVE_ROOTS").is_some() {
        builder = builder.tls_built_in_native_certs(false);
    }

    let result = builder.build().expect("build client").get(url).send().await;
    if expected == "success" {
        let response = result.expect("trusted localhost certificate should succeed");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "tls-isolation-ok");
        println!("verified: HTTP 200 and fixture response body");
    } else {
        let error = result.expect_err("TLS certificate should be rejected");
        let mut details = format!("{error:?}");
        let mut source = error.source();
        while let Some(cause) = source {
            details.push_str(&format!("\n{cause:?}"));
            source = cause.source();
        }
        // A connection failure, timeout, or unrelated error must not count as a pass.
        assert!(
            details.contains(&expected),
            "expected TLS error {expected}, got: {details}"
        );
        println!("verified TLS rejection: {details}");
    }
}
