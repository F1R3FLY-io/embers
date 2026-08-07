//! Contract tests: drive every stub endpoint with the **real** `openai-api-rs`
//! client (the version f1r3node's Rholang interpreter uses) pointed at an
//! in-process stub via `OPENAI_API_BASE`, asserting each response deserializes
//! through that client. Fidelity is thus structural — the stub cannot drift from
//! the client's wire types without a test failing.
//!
//! All traffic is loopback (`127.0.0.1`, ephemeral ports).

use std::net::SocketAddr;
use std::sync::Mutex;

use openai_api_rs::v1::api::OpenAIClient;
use openai_api_rs::v1::audio::AudioSpeechRequest;
use openai_api_rs::v1::chat_completion::{
    ChatCompletionMessage,
    ChatCompletionRequest,
    Content,
    MessageRole,
};
use openai_api_rs::v1::image::ImageGenerationRequest;
use openai_stub::OpenAiStub;

/// `OpenAIClient::build()` reads `OPENAI_API_BASE` from the process environment.
/// This lock serializes the env write together with the build that reads it, so
/// concurrent tests (each with its own stub on its own port) never observe each
/// other's base URL. Once built, the client owns its endpoint, so the lock need
/// only be held across `build`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Build a real client whose `/v1/...` calls reach `stub`, exactly as f1r3node
/// builds it (`OPENAI_API_BASE` + `OpenAIClient::builder().build()`).
fn client_for(stub: &OpenAiStub) -> OpenAIClient {
    let base = stub.base_url();
    let _guard = ENV_LOCK.lock().expect("env lock is not poisoned");
    // SAFETY: every write to `OPENAI_API_BASE` in this test binary, and every
    // `OpenAIClient::build()` that reads it, is serialized by `ENV_LOCK`, so no
    // other thread accesses the environment concurrently with this mutation.
    unsafe {
        std::env::set_var("OPENAI_API_BASE", &base);
    }
    OpenAIClient::builder()
        .with_api_key("stub-key")
        .build()
        .expect("real openai-api-rs client builds against the stub")
}

#[tokio::test]
async fn models_endpoint_lists_three_models_via_real_client() {
    let stub = OpenAiStub::start().await.expect("stub starts on loopback");
    let mut client = client_for(&stub);

    let response = client
        .list_models()
        .await
        .expect("list_models response deserializes through the real client");

    let ids: Vec<String> = response.data.into_iter().filter_map(|model| model.id).collect();
    assert!(ids.contains(&"gpt-4o-mini".to_owned()), "ids = {ids:?}");
    assert!(ids.contains(&"dall-e-3".to_owned()), "ids = {ids:?}");
    assert!(ids.contains(&"tts-1".to_owned()), "ids = {ids:?}");

    stub.shutdown().await;
}

#[tokio::test]
async fn chat_completion_echoes_last_user_message_via_real_client() {
    let stub = OpenAiStub::start().await.expect("stub starts on loopback");
    let mut client = client_for(&stub);

    let request = ChatCompletionRequest::new(
        "gpt-4o-mini".to_owned(),
        vec![ChatCompletionMessage {
            role: MessageRole::user,
            content: Content::Text("Hello, stub!".to_owned()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
    );

    let response = client
        .chat_completion(request)
        .await
        .expect("chat_completion response deserializes through the real client");

    assert_eq!(response.object, "chat.completion");
    let choice = response.choices.first().expect("one choice");
    assert_eq!(
        choice.message.content.as_deref(),
        Some("[stub:gpt-4o-mini] Hello, stub!")
    );
    assert!(
        response.id.expect("id is present").starts_with("chatcmpl-stub-"),
        "id should be a stub chat-completion id"
    );
    assert_eq!(response.usage.total_tokens, 0);

    stub.shutdown().await;
}

#[tokio::test]
async fn image_generation_returns_served_loopback_png_via_real_client() {
    let stub = OpenAiStub::start().await.expect("stub starts on loopback");
    let mut client = client_for(&stub);

    let request = ImageGenerationRequest::new("a red apple".to_owned())
        .model("dall-e-3".to_owned())
        .n(1)
        .size("1024x1024".to_owned())
        .response_format("url".to_owned());

    let response = client
        .image_generation(request)
        .await
        .expect("image_generation response deserializes through the real client");

    let url = response.data.first().expect("one image").url.clone();
    assert!(url.starts_with("http://127.0.0.1:"), "url = {url}");
    assert!(
        std::path::Path::new(&url)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png")),
        "url = {url}"
    );

    // The URL is a real loopback asset: fetch it and verify the PNG signature.
    let bytes = reqwest::get(&url)
        .await
        .expect("fetch the served PNG over loopback")
        .bytes()
        .await
        .expect("read PNG bytes");
    assert_eq!(
        &bytes[..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
        "served asset is a PNG"
    );

    stub.shutdown().await;
}

#[tokio::test]
async fn audio_speech_writes_valid_wav_via_real_client() {
    let stub = OpenAiStub::start().await.expect("stub starts on loopback");
    let mut client = client_for(&stub);

    // The client writes the response bytes to this path; use a port-unique name
    // so parallel tests never collide on the same file.
    let output = std::env::temp_dir().join(format!(
        "openai-stub-contract-{}.wav",
        stub.addr().port()
    ));
    let request = AudioSpeechRequest::new(
        "tts-1".to_owned(),
        "Hello, speech".to_owned(),
        "shimmer".to_owned(),
        output.to_string_lossy().into_owned(),
    );

    let response = client
        .audio_speech(request)
        .await
        .expect("audio_speech round-trips through the real client");
    assert!(response.result, "client reports the speech write succeeded");

    let bytes = std::fs::read(&output).expect("read the WAV the client wrote");
    let _ = std::fs::remove_file(&output);
    assert_eq!(&bytes[0..4], b"RIFF", "WAV RIFF magic");
    assert_eq!(&bytes[8..12], b"WAVE", "WAV WAVE magic");

    stub.shutdown().await;
}

#[tokio::test]
async fn request_ring_buffer_records_received_requests() {
    let stub = OpenAiStub::start().await.expect("stub starts on loopback");
    let mut client = client_for(&stub);

    let _ = client.list_models().await.expect("list_models");

    // Visible through the library handle...
    let recorded = stub.requests();
    assert!(
        recorded
            .iter()
            .any(|request| request.method == "GET" && request.path == "/v1/models"),
        "recorded = {recorded:?}"
    );

    // ...and through the loopback `/stub/requests` endpoint.
    let via_http: serde_json::Value = reqwest::get(format!("{}/stub/requests", stub.origin()))
        .await
        .expect("GET /stub/requests")
        .json()
        .await
        .expect("stub/requests returns JSON");
    assert!(
        via_http
            .as_array()
            .expect("an array of requests")
            .iter()
            .any(|request| request["path"] == "/v1/models"),
        "via_http = {via_http}"
    );

    stub.shutdown().await;
}

#[tokio::test]
async fn refuses_non_loopback_bind_address() {
    let public: SocketAddr = "8.8.8.8:8571".parse().expect("parse addr");
    let result = OpenAiStub::start_on(public).await;
    assert!(result.is_err(), "the stub must refuse non-loopback binds");
}
