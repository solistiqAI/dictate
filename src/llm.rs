//! A local language model for "Korrigieren" (`correct.rs`): Gemma 4 E4B through llama.cpp on the
//! GPU (Metal; the kernels are embedded as source and compiled at load, no Xcode needed).
//!
//! The model loads once on its own thread and stays loaded while Dictate runs. The system prompt is
//! evaluated once and kept in the KV cache; each request only evaluates its own text. Greedy
//! decoding, at most about 1.5 times the input in tokens.

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;

/// A text model `dictate model download` can fetch.
pub struct TextModel {
    pub name: &'static str,
    pub label: &'static str,
    /// The file on Hugging Face.
    pub url: &'static str,
    /// Its name in the model folder (says whose build it is: builds differ a lot).
    pub file: &'static str,
}

/// lmstudio-community's Q4_K_M keeps the per-layer and token embeddings at Q6_K; unsloth's Q4_K_M
/// (Q5_K / Q4_K embeddings) left misheard corrections like "um drei, alle um fünf" alone that
/// Ollama's build (BF16 embeddings) and this one fix: eval/corrections.tsv 24/26 corrected,
/// 18/19 left alone vs unsloth 22/26 (s604-t6). Google's QAT q4_0 was worse at leaving text alone.
pub const MODELS: &[TextModel] = &[TextModel {
    name: "gemma-4-e4b",
    label: "Gemma 4 E4B (lokal auf der GPU, empfohlen, 5 GB)",
    url: "https://huggingface.co/lmstudio-community/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf",
    file: "gemma-4-E4B-it-Q4_K_M.lmstudio.gguf",
}];

pub const DEFAULT: &str = crate::config::DEFAULT_CORRECT_MODEL;

pub fn find(name: &str) -> Option<&'static TextModel> {
    MODELS.iter().find(|m| m.name == name)
}

pub fn path(model: &TextModel) -> PathBuf {
    // Another GGUF for comparisons (`DICTATE_LLM_FILE=…`), never in the app.
    if let Some(file) = std::env::var_os("DICTATE_LLM_FILE") {
        return file.into();
    }
    crate::asr::model_dir(model.name).join(model.file)
}

/// Set while a download runs (two curls on one `.part` would garble it).
pub static DOWNLOADING: AtomicBool = AtomicBool::new(false);

/// Downloads `name` into the models folder (resumable, like the speech models).
pub fn download(name: &str) -> Result<PathBuf> {
    let model = find(name).ok_or_else(|| anyhow!("unknown text model {name:?}"))?;
    let target = path(model);
    if target.exists() {
        return Ok(target);
    }
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        bail!("already downloading");
    }
    let result = fetch(model, &target);
    DOWNLOADING.store(false, Ordering::SeqCst);
    result.map(|()| target)
}

fn fetch(model: &TextModel, target: &std::path::Path) -> Result<()> {
    let name = model.name;
    let dir = target.parent().unwrap();
    std::fs::create_dir_all(dir)?;
    let part = dir.join(format!("{}.part", model.file));
    let url = model.url;
    eprintln!("downloading {name}/{}", model.file);
    let status = Command::new("curl")
        .args(["-fL", "--progress-bar", "-C", "-", "-o"])
        .arg(&part)
        .arg(url)
        .status()
        .context("run curl")?;
    if !status.success() {
        bail!("download of {url} failed ({status}); run it again to resume");
    }
    std::fs::rename(&part, target)?;
    Ok(())
}

struct Request {
    texts: Vec<String>,
    reply: Sender<Result<Vec<String>>>,
}

/// A model loading or loaded on its own thread.
#[derive(Clone)]
pub struct Llm {
    tx: Sender<Request>,
}

impl Llm {
    /// Starts loading `model` with `system` as its standing instructions; requests wait for it.
    pub fn start(model: &'static TextModel, system: &str) -> Llm {
        let (tx, rx) = channel::<Request>();
        let system = system.to_string();
        std::thread::Builder::new()
            .name("llm".into())
            .spawn(move || serve(model, &system, rx))
            .expect("spawn llm thread");
        Llm { tx }
    }

    /// Answers to several user turns at once: they run side by side in one batch, so the time is
    /// about that of the longest, not the sum.
    pub fn ask_all(&self, texts: &[String], timeout: Duration) -> Result<Vec<String>> {
        let (reply, answer) = channel();
        self.tx
            .send(Request {
                texts: texts.to_vec(),
                reply,
            })
            .map_err(|_| anyhow!("model thread ended"))?;
        answer
            .recv_timeout(timeout)
            .map_err(|_| anyhow!("no answer within {} s", timeout.as_secs()))?
    }
}

/// Set at process exit: model threads free their GPU memory and end.
static EXITING: AtomicBool = AtomicBool::new(false);
/// Model threads still holding a model.
static LIVE: AtomicUsize = AtomicUsize::new(0);

/// At `exit()`, before ggml's own teardown: llama.cpp's Metal device asserts (and the process
/// crashes) when it is freed while a model still holds buffers on it. Registered after the first
/// load, so it runs before the teardown ggml registered while loading.
extern "C" fn free_at_exit() {
    EXITING.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(3);
    while LIVE.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn serve(model: &'static TextModel, system: &str, rx: Receiver<Request>) {
    LIVE.fetch_add(1, Ordering::SeqCst);
    run(model, system, &rx);
    LIVE.fetch_sub(1, Ordering::SeqCst);
}

/// Answers requests until every handle is gone or the process exits; model and context live on
/// this stack and are freed on return.
fn run(model: &'static TextModel, system: &str, rx: &Receiver<Request>) {
    let started = Instant::now();
    let loaded = load(model);
    if loaded.is_ok() {
        eprintln!(
            "{}: weights loaded in {:.1} s",
            model.name,
            started.elapsed().as_secs_f32()
        );
    }
    let mut engine = match &loaded {
        Ok((backend, weights)) => Engine::new(model, backend, weights, system),
        Err(e) => Err(anyhow!("{e:#}")),
    };
    match &engine {
        Ok(_) => {
            static HOOK: std::sync::Once = std::sync::Once::new();
            HOOK.call_once(|| unsafe {
                libc::atexit(free_at_exit);
            });
        }
        Err(e) => eprintln!("{}: {e:#}", model.name),
    }
    while !EXITING.load(Ordering::SeqCst) {
        let request = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(request) => request,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let answer = match &mut engine {
            Ok(engine) => engine.answer_all(&request.texts),
            Err(e) => Err(anyhow!("{e:#}")),
        };
        let _ = request.reply.send(answer);
    }
    // The context before the model it borrows.
    drop(engine);
}

/// Room for the instructions and up to `SEQUENCES` texts with their answers, side by side.
const CONTEXT: u32 = 8192;
/// Texts answered in one batch (each its own sequence, all sharing the cached instructions).
const SEQUENCES: usize = 16;

fn load(model: &TextModel) -> Result<(LlamaBackend, LlamaModel)> {
    let file = path(model);
    if !file.exists() {
        bail!("not downloaded (`dictate model download {}`)", model.name);
    }
    // Quiet from the start: Metal's device setup alone logs some fifty lines.
    llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default().with_logs_enabled(false));
    let backend = LlamaBackend::init()?;
    let params = LlamaModelParams::default().with_n_gpu_layers(999);
    let weights = LlamaModel::load_from_file(&backend, &file, &params)
        .with_context(|| format!("load {}", file.display()))?;
    Ok((backend, weights))
}

struct Engine<'m> {
    model: &'m LlamaModel,
    context: llama_cpp_2::context::LlamaContext<'m>,
    /// Tokens of the system turn, kept in the cache.
    prefix: usize,
}

impl<'m> Engine<'m> {
    fn new(
        about: &TextModel,
        backend: &LlamaBackend,
        model: &'m LlamaModel,
        system: &str,
    ) -> Result<Engine<'m>> {
        let started = Instant::now();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32;
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(CONTEXT))
            .with_n_batch(CONTEXT)
            .with_n_threads(threads)
            .with_n_threads_batch(threads)
            // Gemma's sliding-window layers forget old positions otherwise, and the cached system
            // turn could not be kept while each request's tokens are dropped.
            .with_swa_full(true)
            // One cache for all sequences: the instructions are shared, not copied per sequence.
            .with_n_seq_max(SEQUENCES as u32)
            .with_kv_unified(true);
        let context = model.new_context(backend, params)?;
        let gpu = llama_cpp_2::list_llama_ggml_backend_devices()
            .into_iter()
            .find(|d| d.backend != "CPU")
            .map_or("CPU only".to_string(), |d| d.description);
        let mut engine = Engine {
            model,
            context,
            prefix: 0,
        };
        let turn = format!("<bos><|turn>system\n{}<turn|>\n", system.trim());
        let tokens = engine.tokens(&turn);
        if std::env::var_os("DICTATE_LLM_DEBUG").is_some() {
            let vocab = model.vocab();
            let shown: Vec<String> = tokens[..6.min(tokens.len())]
                .iter()
                .map(|t| {
                    format!(
                        "{}={:?}",
                        t.0,
                        String::from_utf8_lossy(&vocab.token_to_piece(*t, true, None))
                    )
                })
                .collect();
            eprintln!(
                "llm debug: bos {} first tokens {}",
                vocab.bos().0,
                shown.join(" ")
            );
        }
        engine.eval(&tokens, 0, false)?;
        engine.prefix = tokens.len();
        eprintln!(
            "{}: ready, {:.1} GB weights on {gpu}, {} instruction tokens cached in {:.1} s",
            about.name,
            model.size() as f64 / 1e9,
            engine.prefix,
            started.elapsed().as_secs_f32(),
        );
        Ok(engine)
    }

    fn tokens(&self, text: &str) -> Vec<llama_cpp_2::token::LlamaToken> {
        self.model.vocab().tokenize(text.as_bytes(), false, true)
    }

    /// Evaluates `tokens` from position `at`; with `logits` the last one can be sampled.
    fn eval(
        &mut self,
        tokens: &[llama_cpp_2::token::LlamaToken],
        at: usize,
        logits: bool,
    ) -> Result<()> {
        let mut batch = LlamaBatch::new(tokens.len().max(1), 1);
        for (i, token) in tokens.iter().enumerate() {
            batch.add(
                *token,
                (at + i) as i32,
                &[0],
                logits && i + 1 == tokens.len(),
            )?;
        }
        self.context.decode(&mut batch)?;
        Ok(())
    }

    fn answer_all(&mut self, texts: &[String]) -> Result<Vec<String>> {
        let mut answers = Vec::with_capacity(texts.len());
        for group in texts.chunks(SEQUENCES) {
            answers.extend(self.answer_group(group)?);
        }
        Ok(answers)
    }

    /// Up to `SEQUENCES` texts, greedy, side by side: sequence `s` answers `texts[s]`.
    fn answer_group(&mut self, texts: &[String]) -> Result<Vec<String>> {
        let prefix = self.prefix as u32;
        // Back to the cached instructions, shared by every sequence.
        self.context
            .clear_kv_cache_seq(Some(0), Some(prefix), None)?;
        for seq in 1..SEQUENCES as u32 {
            self.context.clear_kv_cache_seq(Some(seq), None, None)?;
        }
        for seq in 1..texts.len() as i32 {
            self.context.copy_kv_cache_seq(0, seq, None, Some(prefix))?;
        }
        let turns: Vec<_> = texts
            .iter()
            .map(|t| self.tokens(&format!("<|turn>user\n{}<turn|>\n<|turn>model\n", t.trim())))
            .collect();
        let total: usize = turns.iter().map(Vec::len).sum();
        let mut limits: Vec<usize> = turns.iter().map(|t| t.len() * 3 / 2 + 16).collect();
        if self.prefix + total + limits.iter().sum::<usize>() > CONTEXT as usize {
            bail!("text too long for the model");
        }
        let mut batch = LlamaBatch::new(total.max(texts.len()), SEQUENCES as i32);
        // Where each sequence's logits are in the batch, and its next position.
        let mut at: Vec<i32> = vec![0; texts.len()];
        let mut pos: Vec<usize> = vec![self.prefix; texts.len()];
        for (seq, turn) in turns.iter().enumerate() {
            for (i, token) in turn.iter().enumerate() {
                let last = i + 1 == turn.len();
                if last {
                    at[seq] = batch.n_tokens();
                }
                batch.add(*token, (self.prefix + i) as i32, &[seq as i32], last)?;
            }
            pos[seq] += turn.len();
        }
        self.context.decode(&mut batch)?;
        let mut sampler = LlamaSampler::greedy();
        let vocab = self.model.vocab();
        let mut out: Vec<Vec<u8>> = vec![Vec::new(); texts.len()];
        let mut active: Vec<bool> = vec![true; texts.len()];
        loop {
            batch.clear();
            for seq in 0..texts.len() {
                if !active[seq] {
                    continue;
                }
                let token = sampler.sample(&self.context, at[seq]);
                let text = &mut out[seq];
                if !vocab.is_eog(token) {
                    text.extend(vocab.token_to_piece(token, true, None));
                }
                // Gemma 4 ends its turn with "<turn|>"; not every build marks it end-of-generation.
                let ended = vocab.is_eog(token) || text.ends_with(b"<turn|>");
                limits[seq] -= 1;
                if ended || limits[seq] == 0 {
                    active[seq] = false;
                    continue;
                }
                at[seq] = batch.n_tokens();
                batch.add(token, pos[seq] as i32, &[seq as i32], true)?;
                pos[seq] += 1;
            }
            if batch.n_tokens() == 0 {
                break;
            }
            self.context.decode(&mut batch)?;
        }
        if std::env::var_os("DICTATE_LLM_DEBUG").is_some() {
            for o in &out {
                eprintln!("llm debug: raw answer {:?}", String::from_utf8_lossy(o));
            }
        }
        Ok(out
            .iter()
            .map(|o| clean(&String::from_utf8_lossy(o)))
            .collect())
    }
}

/// The answer without a thinking channel and stray turn markers.
fn clean(answer: &str) -> String {
    let mut text = answer.to_string();
    while let (Some(start), Some(end)) = (text.find("<|channel>"), text.find("<channel|>")) {
        if end < start {
            break;
        }
        text.replace_range(start..end + "<channel|>".len(), "");
    }
    text.replace("<turn|>", "").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_lose_thinking_and_markers() {
        assert_eq!(
            clean("<|channel>thought\n<channel|>Wir treffen uns um fünf.<turn|>"),
            "Wir treffen uns um fünf."
        );
        assert_eq!(clean("  Text\n"), "Text");
    }

    #[test]
    fn the_model_has_a_home() {
        let model = find(DEFAULT).unwrap();
        assert!(path(model).ends_with("models/gemma-4-e4b/gemma-4-E4B-it-Q4_K_M.lmstudio.gguf"));
    }
}
