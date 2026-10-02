#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests"
)]

use std::time::Duration;

use secrecy::SecretString;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;
use crate::testing::{self, Rules, TestServer};

#[test]
fn endpoints() {
    assert_eq!(endpoint_url("").unwrap(), None);
    assert_eq!(
        endpoint_url("minio.lan:9000/").unwrap().as_deref(),
        Some("https://minio.lan:9000")
    );
    assert_eq!(
        endpoint_url("http://127.0.0.1:9000").unwrap().as_deref(),
        Some("http://127.0.0.1:9000")
    );
    assert!(endpoint_url("ftp://x").is_err());
    assert!(endpoint_url("https://").is_err());
    assert!(endpoint_url("https://s3.example/bucket").is_err());
}

#[test]
fn names_keys_and_types() {
    assert!(is_bucket_name("my-bucket.2026"));
    for bad in ["ab", "My-Bucket", "-bucket", "bucket-", "a_b_c"] {
        assert!(!is_bucket_name(bad), "{bad}");
    }
    assert_eq!(encode_key("a b/ñ+%.txt"), "a%20b/%C3%B1%2B%25.txt");
    assert_eq!(
        content_type("site/index.HTML"),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(content_type("archive.tar.zst"), None);
    let spec = S3Spec {
        endpoint: String::new(),
        region: String::new(),
        path_style: true,
        access_key: "AKID".into(),
        secret_key: SecretString::from("very secret".to_owned()),
    };
    assert!(!format!("{spec:?}").contains("very secret"));
}

async fn server(rules: Rules) -> (TestServer, S3) {
    let server = testing::serve(rules).await.unwrap();
    let s3 = S3::new(&server.spec()).unwrap();
    (server, s3)
}

async fn read_all(s3: &S3, bucket: &str, key: &str, offset: u64) -> Vec<u8> {
    let mut reader = Box::pin(s3.read(bucket, key, offset).await.unwrap());
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.unwrap();
    bytes
}

#[tokio::test(flavor = "multi_thread")]
async fn buckets_folders_and_objects() {
    let (server, s3) = server(Rules::default()).await;
    s3.create_bucket("photos").await.unwrap();
    assert!(matches!(
        s3.create_bucket("photos").await,
        Err(S3Error::Exists { .. })
    ));
    assert!(matches!(
        s3.create_bucket("Bad_Name").await,
        Err(S3Error::Invalid(_))
    ));
    assert_eq!(
        s3.buckets()
            .await
            .unwrap()
            .into_iter()
            .map(|bucket| bucket.name)
            .collect::<Vec<_>>(),
        ["photos"]
    );
    assert!(s3.bucket_exists("photos").await.unwrap());
    assert!(!s3.bucket_exists("nope").await.unwrap());
    for key in ["a/", "a/b.txt", "a/c/d.txt", "e f.txt"] {
        s3.put("photos", key, key.as_bytes().to_vec())
            .await
            .unwrap();
    }
    let top = s3.list("photos", "").await.unwrap();
    assert_eq!(top.folders, ["a/"]);
    assert_eq!(
        top.objects
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["e f.txt"]
    );
    let inside = s3.list("photos", "a/").await.unwrap();
    assert_eq!(inside.folders, ["a/c/"]);
    assert_eq!(
        inside
            .objects
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["a/", "a/b.txt"]
    );
    assert_eq!(s3.list_all("photos", "a/").await.unwrap().len(), 3);
    assert!(s3.has_prefix("photos", "a/c/").await.unwrap());
    assert!(!s3.has_prefix("photos", "z/").await.unwrap());
    let head = s3.head("photos", "a/b.txt").await.unwrap().unwrap();
    assert_eq!(head.size, 7);
    assert!(head.modified.is_some());
    assert_eq!(s3.head("photos", "missing").await.unwrap(), None);
    assert_eq!(read_all(&s3, "photos", "e f.txt", 0).await, b"e f.txt");
    assert_eq!(read_all(&s3, "photos", "e f.txt", 3).await, b".txt");
    // Copy, delete, delete many.
    s3.copy("photos", "e f.txt", "photos", "copy/e f.txt", 7)
        .await
        .unwrap();
    assert_eq!(server.object("photos", "copy/e f.txt").unwrap(), b"e f.txt");
    s3.delete("photos", "e f.txt").await.unwrap();
    assert!(matches!(
        s3.delete_bucket("photos").await,
        Err(S3Error::Failed { .. })
    ));
    let keys = server.keys("photos");
    s3.delete_many("photos", &keys).await.unwrap();
    assert!(server.keys("photos").is_empty());
    s3.delete_bucket("photos").await.unwrap();
    assert!(s3.buckets().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn listings_come_in_pages() {
    let (_server, s3) = server(Rules {
        buckets: vec!["b".into()],
        page_size: 2,
    })
    .await;
    for key in ["1", "2", "3", "d/x", "d/y", "e/z", "f"] {
        s3.put("b", key, Vec::new()).await.unwrap();
    }
    let listing = s3.list("b", "").await.unwrap();
    assert_eq!(listing.folders, ["d/", "e/"]);
    assert_eq!(
        listing
            .objects
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["1", "2", "3", "f"]
    );
    assert_eq!(s3.list_all("b", "").await.unwrap().len(), 7);
}

#[tokio::test(flavor = "multi_thread")]
async fn uploads_small_and_multipart() {
    let (server, s3) = server(Rules {
        buckets: vec!["b".into()],
        ..Rules::default()
    })
    .await;
    // Small: one request.
    let mut writer = s3.writer("b", "small.txt", 1024);
    writer.write_all(b"hello").await.unwrap();
    writer.shutdown().await.unwrap();
    assert_eq!(server.object("b", "small.txt").unwrap(), b"hello");
    // Bigger than a part: three and a half parts, written in odd pieces.
    let data: Vec<u8> = (0..3584_u32).map(|i| (i % 251) as u8).collect();
    let mut writer = s3.writer("b", "big.bin", 1024);
    for chunk in data.chunks(300) {
        writer.write_all(chunk).await.unwrap();
    }
    writer.shutdown().await.unwrap();
    assert_eq!(server.object("b", "big.bin").unwrap(), data);
    assert_eq!(server.uploads_in_progress(), 0);
    assert_eq!(read_all(&s3, "b", "big.bin", 3000).await, &data[3000..]);
    // Dropped before the end: nothing is left.
    let mut writer = s3.writer("b", "dropped.bin", 1024);
    writer.write_all(&data).await.unwrap();
    drop(writer);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while server.uploads_in_progress() > 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the upload wasn't aborted"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(server.object("b", "dropped.bin"), None);
    // An empty file.
    let mut writer = s3.writer("b", "empty", 1024);
    writer.shutdown().await.unwrap();
    assert_eq!(server.object("b", "empty").unwrap(), b"");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upload_into_a_missing_bucket_says_why() {
    let (_server, s3) = server(Rules::default()).await;
    let mut writer = s3.writer("nope", "x", 1024);
    writer.write_all(b"data").await.unwrap();
    let error = writer.shutdown().await.unwrap_err();
    assert!(error.to_string().contains("not found"), "{error}");
}

#[tokio::test(flavor = "multi_thread")]
async fn temporary_links_work_without_the_keys() {
    let (server, s3) = server(Rules {
        buckets: vec!["b".into()],
        ..Rules::default()
    })
    .await;
    s3.put("b", "docs/a b.txt", b"shared".to_vec())
        .await
        .unwrap();
    let link = s3
        .presign_get("b", "docs/a b.txt", Duration::from_secs(3600))
        .await
        .unwrap();
    assert!(
        link.starts_with(&format!("{}/b/docs/a%20b.txt?", server.endpoint())),
        "{link}"
    );
    assert!(link.contains("X-Amz-Expires=3600"), "{link}");
    assert!(!link.contains(testing::SECRET_KEY), "{link}");
    // Fetched with no keys at all.
    let path = link.trim_start_matches(&server.endpoint()).to_owned();
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", server.port))
        .await
        .unwrap();
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.ends_with("shared"), "{response}");
    assert!(matches!(
        s3.presign_get("b", "x", Duration::from_secs(8 * 24 * 3600))
            .await,
        Err(S3Error::Invalid(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_keys_and_no_server() {
    let (server, _) = server(Rules::default()).await;
    let wrong = S3::new(&S3Spec {
        access_key: "SOMEONEELSE".into(),
        ..server.spec()
    })
    .unwrap();
    assert!(matches!(wrong.buckets().await, Err(S3Error::Denied { .. })));
    // A port nothing listens on.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let nobody = S3::new(&S3Spec {
        endpoint: format!("http://127.0.0.1:{port}"),
        ..server.spec()
    })
    .unwrap();
    let error = nobody.buckets().await.unwrap_err();
    assert!(error.is_transient(), "{error:?}");
}
