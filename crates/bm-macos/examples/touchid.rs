//! Spike 4: Touch ID availability, Keychain round-trip, optional prompt.
//!
//! `cargo run -p bm-macos --example touchid [-- --prompt | --store | --load | --delete]`
//!
//! `--store` then `--load` from a *rebuilt* binary (different `BM_BUILD_TAG`)
//! checks that the Keychain ACL survives rebuilds when both are signed with the
//! same identity (scripts/create-signing-cert.sh).

use bm_macos::{biometrics, keychain};

const SERVICE: &str = "app.brainmatter.spike";
const ACCOUNT: &str = "touchid-key";
const BUILD_TAG: Option<&str> = option_env!("BM_BUILD_TAG");

fn secret() -> Vec<u8> {
    (0u8..32).map(|i| i.wrapping_mul(37) ^ 0xa5).collect()
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    println!("build tag: {BUILD_TAG:?}");
    match mode.as_str() {
        "--store" => {
            keychain::store_secret(SERVICE, ACCOUNT, &secret()).expect("store");
            println!("stored");
        }
        "--load" => match keychain::load_secret(SERVICE, ACCOUNT) {
            Ok(loaded) => println!("loaded matches: {}", loaded.as_deref() == Some(&secret()[..])),
            Err(e) => println!("load failed: {e:?}"),
        },
        "--delete" => println!("delete: {:?}", keychain::delete_secret(SERVICE, ACCOUNT)),
        "--prompt" => {
            println!("waiting for Touch ID…");
            println!(
                "prompt: {:?}",
                biometrics::authenticate("Brainmatter entsperren (Spike-Test)")
            );
        }
        _ => {
            println!("biometry: {:?}", biometrics::available());
            keychain::store_secret(SERVICE, ACCOUNT, &secret()).expect("store");
            let loaded = keychain::load_secret(SERVICE, ACCOUNT).expect("load");
            println!("keychain round-trip: {}", loaded.as_deref() == Some(&secret()[..]));
            keychain::delete_secret(SERVICE, ACCOUNT).expect("delete");
        }
    }
}
