#[cfg(feature = "burn")]
use crate::burn_backend::tdt::{TdtDecoderJoint, TdtEncoder};
use crate::error::{Error, Result};
use crate::execution::ModelConfig as ExecutionConfig;
#[cfg(feature = "ort")]
use ndarray::Array1;
use ndarray::{Array2, Array3};
#[cfg(feature = "ort")]
use ort::session::Session;
use std::path::{Path, PathBuf};

/// TDT model configs
#[derive(Debug, Clone)]
pub struct TDTModelConfig {
    pub vocab_size: usize,
}

impl TDTModelConfig {
    /// Create config with specified vocab size
    pub fn new(vocab_size: usize) -> Self {
        Self { vocab_size }
    }
}

pub struct ParakeetTDTModel {
    encoder: Encoder,
    decoder_joint: DecoderJoint,
    config: TDTModelConfig,
    speaker_targets: Option<(Vec<f32>, Vec<f32>)>,
    /// Tokens the greedy decoder may emit (dictate patch); `None` = all.
    pub(crate) allowed_tokens: Option<Vec<bool>>,
    /// Context biasing (dictate patch); `None` = plain greedy decoding.
    pub(crate) boost: Option<TokenBoost>,
}

/// Context biasing for greedy decoding (dictate patch): a token automaton over the words to
/// favour. Node 0 is the start; `next[node]` lists the transitions out of a node, sorted by token.
/// While decoding sits on a node, each listed token gets its `bonus` added to its logit, so a word
/// that is under way is more likely to be finished. An emitted token follows its transition, or
/// the start node's when the node has none, or goes back to the start (path broken).
///
/// The bonus only decides between tokens: it never turns a blank into a token, so it cannot add
/// words. A `new_word` transition (one that starts a word to favour rather than continuing one)
/// only competes when the model's own choice starts a word too (`word_start`), so it changes which
/// word begins, not where words break or how numbers are written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenBoost {
    pub next: Vec<Vec<BoostEdge>>,
    /// Per token id: does it begin a word (`▁` plus at least one character)?
    pub word_start: Vec<bool>,
}

/// One transition of a [`TokenBoost`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoostEdge {
    pub token: usize,
    pub to: usize,
    pub bonus: f32,
    pub new_word: bool,
}

impl TokenBoost {
    fn edge(&self, node: usize, token: usize) -> Option<&BoostEdge> {
        let edges = self.edges(node);
        edges
            .binary_search_by_key(&token, |e| e.token)
            .ok()
            .map(|i| &edges[i])
    }

    /// The node after emitting `token` on `node`.
    pub fn step(&self, node: usize, token: usize) -> usize {
        self.edge(node, token)
            .or_else(|| self.edge(0, token))
            .map_or(0, |e| e.to)
    }

    fn edges(&self, node: usize) -> &[BoostEdge] {
        self.next.get(node).map_or(&[], Vec::as_slice)
    }

    /// The bonus `token` gets on `node` when the model's own choice is `leader`.
    fn bonus(&self, node: usize, token: usize, leader: usize) -> f32 {
        match self.edge(node, token) {
            Some(e) if !e.new_word || self.word_start.get(leader).copied().unwrap_or(false) => {
                e.bonus
            }
            _ => 0.0,
        }
    }
}

/// The encoder graph, on whichever backend its execution configuration selects.
enum Encoder {
    #[cfg(feature = "ort")]
    Ort(Session),
    #[cfg(feature = "burn")]
    Burn(Box<TdtEncoder>),
}

/// The decoder/joint graph, on whichever backend its execution configuration selects.
enum DecoderJoint {
    #[cfg(feature = "ort")]
    Ort(Session),
    #[cfg(feature = "burn")]
    Burn(Box<TdtDecoderJoint>),
}

impl Encoder {
    fn load(path: &Path, config: &ExecutionConfig) -> Result<Self> {
        #[cfg(feature = "burn")]
        if config.execution_provider.is_burn() {
            return Ok(Self::Burn(Box::new(TdtEncoder::load(
                path,
                config.execution_provider,
            )?)));
        }
        #[cfg(feature = "ort")]
        {
            Ok(Self::Ort(config.build_session(path)?))
        }
        #[cfg(not(feature = "ort"))]
        {
            Err(Error::Config(format!(
                "{:?} needs the `ort` feature",
                config.execution_provider
            )))
        }
    }
}

impl DecoderJoint {
    #[cfg_attr(not(feature = "burn"), allow(unused_variables))]
    fn load(path: &Path, config: &ExecutionConfig, vocab_size: usize) -> Result<Self> {
        #[cfg(feature = "burn")]
        if config.execution_provider.is_burn() {
            return Ok(Self::Burn(Box::new(TdtDecoderJoint::load(
                path,
                config.execution_provider,
                vocab_size,
            )?)));
        }
        #[cfg(feature = "ort")]
        {
            Ok(Self::Ort(config.build_session(path)?))
        }
        #[cfg(not(feature = "ort"))]
        {
            Err(Error::Config(format!(
                "{:?} needs the `ort` feature",
                config.execution_provider
            )))
        }
    }
}

impl ParakeetTDTModel {
    /// Load a TDT model, running the encoder and the decoder/joint under separate execution
    /// configurations.
    ///
    /// # Arguments
    /// * `model_dir` - Directory containing encoder and decoder_joint ONNX files
    /// * `exec_config` - Execution configuration for the encoder session
    /// * `joint_config` - Execution configuration for the decoder/joint session
    /// * `vocab_size` - Vocabulary size (number of tokens including blank)
    pub fn from_pretrained_with_configs<P: AsRef<Path>>(
        model_dir: P,
        exec_config: ExecutionConfig,
        joint_config: ExecutionConfig,
        vocab_size: usize,
    ) -> Result<Self> {
        let model_dir = model_dir.as_ref();

        if vocab_size == 0 {
            return Err(Error::Config("TDT vocabulary is empty".into()));
        }
        let encoder_int8 = !exec_config.execution_provider.is_burn();
        let joint_int8 = !joint_config.execution_provider.is_burn();
        let encoder_path = Self::find_encoder(model_dir, encoder_int8)?;
        let decoder_joint_path = Self::find_decoder_joint(model_dir, joint_int8)?;

        let config = TDTModelConfig::new(vocab_size);

        let encoder = Encoder::load(&encoder_path, &exec_config)?;
        let decoder_joint = DecoderJoint::load(&decoder_joint_path, &joint_config, vocab_size)?;

        Ok(Self {
            encoder,
            decoder_joint,
            config,
            speaker_targets: None,
            allowed_tokens: None,
            boost: None,
        })
    }

    /// Multitalker TDT only: (speaker, everyone else) activity, one value per 80 ms.
    pub fn set_speaker_targets(&mut self, targets: Option<(Vec<f32>, Vec<f32>)>) {
        self.speaker_targets = targets;
    }
    //file names simply from: https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/tree/main
    /// `int8`: whether int8 exports may be picked (burn runs fp32 exports only).
    fn find_encoder(dir: &Path, int8: bool) -> Result<PathBuf> {
        let candidates = [
            "encoder-model.onnx",
            "encoder.onnx",
            "encoder-model.int8.onnx",
        ];
        for candidate in &candidates {
            let path = dir.join(candidate);
            if path.exists() && (int8 || !candidate.contains(".int8.")) {
                return Ok(path);
            }
        }
        // fallback
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|s| s.to_str())
                    && name.starts_with("encoder")
                    && name.ends_with(".onnx")
                    && (int8 || !name.contains(".int8."))
                {
                    return Ok(path);
                }
            }
        }
        Err(Error::Config(format!(
            "No encoder model found in {}",
            dir.display()
        )))
    }

    fn find_decoder_joint(dir: &Path, int8: bool) -> Result<PathBuf> {
        let candidates = [
            "decoder_joint-model.onnx",
            "decoder_joint-model.int8.onnx",
            "decoder_joint.onnx",
            "decoder-model.onnx",
        ];
        for candidate in &candidates {
            let path = dir.join(candidate);
            if path.exists() && (int8 || !candidate.contains(".int8.")) {
                return Ok(path);
            }
        }
        Err(Error::Config(format!(
            "No decoder_joint model found in {}",
            dir.display()
        )))
    }

    /// Run greedy decoding - returns (token_ids, frame_indices, durations)
    pub fn forward(
        &mut self,
        features: Array2<f32>,
    ) -> Result<(Vec<usize>, Vec<usize>, Vec<usize>)> {
        // Run encoder
        let (encoder_out, encoder_len) = self.run_encoder(&features)?;

        // Run greedy decoding with decoder_joint
        let (tokens, frame_indices, durations) = self.greedy_decode(&encoder_out, encoder_len)?;

        Ok((tokens, frame_indices, durations))
    }

    fn run_encoder(&mut self, features: &Array2<f32>) -> Result<(Array3<f32>, i64)> {
        let batch_size = 1;
        let time_steps = features.shape()[0];
        let feature_size = features.shape()[1];

        // TDT encoder expects (batch, features, time) not (batch, time, features)
        let input = features
            .t()
            .to_shape((batch_size, feature_size, time_steps))
            .map_err(|e| Error::Model(format!("Failed to reshape encoder input: {e}")))?
            .to_owned();

        match &mut self.encoder {
            #[cfg(feature = "ort")]
            Encoder::Ort(session) => {
                let input_length = Array1::from_vec(vec![time_steps as i64]);

                let mut inputs: Vec<(String, ort::session::SessionInputValue)> = vec![
                    (
                        "audio_signal".into(),
                        ort::value::Value::from_array(input)?.into(),
                    ),
                    (
                        "length".into(),
                        ort::value::Value::from_array(input_length)?.into(),
                    ),
                ];
                let multitalker = session.inputs().iter().any(|i| i.name() == "spk_targets");
                match (&self.speaker_targets, multitalker) {
                    (Some(_), false) => {
                        return Err(Error::Config(
                            "speaker targets need a multitalker TDT encoder (spk_targets input)"
                                .into(),
                        ));
                    }
                    (targets, true) => {
                        // No targets: one frame, stretched by the encoder to the whole input.
                        let (spk, bg) = targets.clone().unwrap_or((vec![1.0], vec![0.0]));
                        for (name, mask) in [("spk_targets", spk), ("bg_spk_targets", bg)] {
                            let len = mask.len();
                            let mask = Array2::from_shape_vec((1, len), mask)
                                .map_err(|e| Error::Model(format!("{name}: {e}")))?;
                            inputs.push((name.into(), ort::value::Value::from_array(mask)?.into()));
                        }
                    }
                    (None, false) => {}
                }
                let outputs = session.run(inputs)?;

                // TDT encoder outputs [batch, encoder_dim, time] directly
                let encoder_array =
                    crate::tensor_utils::extract_3d_f32(&outputs["outputs"], "encoder output")?;
                let encoded_len = crate::tensor_utils::extract_scalar_i64(
                    &outputs["encoded_lengths"],
                    "encoder lengths",
                )?;

                Ok((encoder_array, encoded_len))
            }
            #[cfg(feature = "burn")]
            Encoder::Burn(encoder) => encoder.run(input, self.speaker_targets.as_ref()),
        }
    }

    /// Run the decoder/joint for one encoder frame `[1, encoder_dim, 1]`: returns the token and
    /// the duration it predicts, and updates the LSTM state when it emits a token.
    fn joint_step(
        &mut self,
        frame: Array3<f32>,
        last_token: i32,
        boost_node: usize,
        state_h: &mut Array3<f32>,
        state_c: &mut Array3<f32>,
    ) -> Result<(usize, usize)> {
        let vocab_size = self.config.vocab_size;
        let blank_id = vocab_size - 1;
        let boost = self.boost.as_ref().map(|b| (b, boost_node));
        match &mut self.decoder_joint {
            #[cfg(feature = "ort")]
            DecoderJoint::Ort(session) => {
                // Current token for prediction network
                let targets = Array2::from_shape_vec((1, 1), vec![last_token])
                    .map_err(|e| Error::Model(format!("Failed to create targets: {e}")))?;

                let outputs = session.run(ort::inputs!(
                    "encoder_outputs" => ort::value::Value::from_array(frame)?,
                    "targets" => ort::value::Value::from_array(targets)?,
                    "target_length" => ort::value::Value::from_array(Array1::from_vec(vec![1i32]))?,
                    "input_states_1" => ort::value::Value::from_array(state_h.clone())?,
                    "input_states_2" => ort::value::Value::from_array(state_c.clone())?
                ))?;

                // Extract logits
                let (_, logits_data) = outputs["outputs"]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| Error::Model(format!("Failed to extract logits: {e}")))?;
                let (token_id, duration_step) =
                    pick(logits_data, vocab_size, self.allowed_tokens.as_deref(), boost);

                if token_id != blank_id {
                    // Update states when we emit a token
                    if let Ok((h_shape, h_data)) =
                        outputs["output_states_1"].try_extract_tensor::<f32>()
                    {
                        let dims = h_shape.as_ref();
                        *state_h = Array3::from_shape_vec(
                            (dims[0] as usize, dims[1] as usize, dims[2] as usize),
                            h_data.to_vec(),
                        )
                        .map_err(|e| Error::Model(format!("Failed to update state_h: {e}")))?;
                    }
                    if let Ok((c_shape, c_data)) =
                        outputs["output_states_2"].try_extract_tensor::<f32>()
                    {
                        let dims = c_shape.as_ref();
                        *state_c = Array3::from_shape_vec(
                            (dims[0] as usize, dims[1] as usize, dims[2] as usize),
                            c_data.to_vec(),
                        )
                        .map_err(|e| Error::Model(format!("Failed to update state_c: {e}")))?;
                    }
                }
                Ok((token_id, duration_step))
            }
            #[cfg(feature = "burn")]
            DecoderJoint::Burn(decoder) => {
                let step = decoder.step(frame.view(), last_token, state_h, state_c)?;
                let (token_id, duration_step) =
                    pick(&step.logits, vocab_size, self.allowed_tokens.as_deref(), boost);
                if token_id != blank_id {
                    *state_h = step.state_h;
                    *state_c = step.state_c;
                }
                Ok((token_id, duration_step))
            }
        }
    }

    fn greedy_decode(
        &mut self,
        encoder_out: &Array3<f32>,
        _encoder_len: i64,
    ) -> Result<(Vec<usize>, Vec<usize>, Vec<usize>)> {
        // encoder_out shape: [batch, encoder_dim, time]
        let encoder_dim = encoder_out.shape()[1];
        let time_steps = encoder_out.shape()[2];
        let vocab_size = self.config.vocab_size;
        let max_tokens_per_step = 10;
        let blank_id = vocab_size - 1;

        // States: (num_layers=2, batch=1, hidden_dim=640)
        let mut state_h = Array3::<f32>::zeros((2, 1, 640));
        let mut state_c = Array3::<f32>::zeros((2, 1, 640));

        let mut tokens = Vec::new();
        let mut frame_indices = Vec::new();
        let mut durations = Vec::new();

        let mut t = 0;
        let mut emitted_tokens = 0;
        let mut last_emitted_token = blank_id as i32;
        let mut boost_node = 0;

        // Frame-by-frame RNN-T/TDT greedy decoding
        while t < time_steps {
            // Get single encoder frame: slice [0, :, t] and reshape to [1, encoder_dim, 1]
            let frame = encoder_out.slice(ndarray::s![0, .., t]).to_owned();
            let frame_reshaped = frame
                .to_shape((1, encoder_dim, 1))
                .map_err(|e| Error::Model(format!("Failed to reshape frame: {e}")))?
                .to_owned();

            // Run decoder_joint
            let (token_id, duration_step) = self.joint_step(
                frame_reshaped,
                last_emitted_token,
                boost_node,
                &mut state_h,
                &mut state_c,
            )?;

            // Check if blank token
            if token_id != blank_id {
                tokens.push(token_id);
                frame_indices.push(t);
                durations.push(duration_step);
                last_emitted_token = token_id as i32;
                emitted_tokens += 1;
                if let Some(boost) = &self.boost {
                    boost_node = boost.step(boost_node, token_id);
                }
            }
            // When duration > 0, skip frames according to duration prediction
            // Otherwise advance by 1 on blank or when max tokens reached
            if duration_step > 0 {
                t += duration_step;
                emitted_tokens = 0;
            } else if token_id == blank_id || emitted_tokens >= max_tokens_per_step {
                t += 1;
                emitted_tokens = 0;
            }
        }

        Ok((tokens, frame_indices, durations))
    }
}

/// The token and the duration a TDT joint output predicts: it holds `vocab_size` token logits
/// followed by the duration logits.
/// dictate patch: `allowed` masks tokens, `boost` adds bonuses (see [`TokenBoost`]).
fn pick(
    logits: &[f32],
    vocab_size: usize,
    allowed: Option<&[bool]>,
    boost: Option<(&TokenBoost, usize)>,
) -> (usize, usize) {
    let argmax = |values: &[f32]| {
        values
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx)
    };
    let (vocab_logits, duration_logits) = logits.split_at(vocab_size.min(logits.len()));
    // dictate patch: tokens outside `allowed` never win (blank always may).
    let token_id = match allowed {
        Some(allowed) => vocab_logits
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx == vocab_size - 1 || allowed.get(*idx).copied().unwrap_or(true))
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx),
        None => argmax(vocab_logits),
    }
    .unwrap_or(vocab_size - 1);
    // dictate patch: context biasing, among tokens only (a blank stays a blank).
    let token_id = match boost {
        Some((boost, node)) if token_id != vocab_size - 1 => {
            let leader = token_id;
            let mut best = (leader, vocab_logits[leader] + boost.bonus(node, leader, leader));
            for edge in boost.edges(node) {
                let t = edge.token;
                let ok = t < vocab_size - 1
                    && allowed.is_none_or(|a| a.get(t).copied().unwrap_or(true));
                if ok && let Some(&logit) = vocab_logits.get(t) {
                    let score = logit + boost.bonus(node, t, leader);
                    if score > best.1 {
                        best = (t, score);
                    }
                }
            }
            best.0
        }
        _ => token_id,
    };
    let duration_step = argmax(duration_logits).unwrap_or(0);
    (token_id, duration_step)
}

#[cfg(test)]
mod tests {
    use super::{BoostEdge, TokenBoost, pick};

    #[test]
    fn pick_splits_token_and_duration_logits() {
        // vocabulary of 3, then 2 duration logits
        assert_eq!(pick(&[0.1, 0.9, 0.2, 0.3, 0.7], 3, None, None), (1, 1));
        // no duration logits: duration 0
        assert_eq!(pick(&[0.1, 0.2, 0.9], 3, None, None), (2, 0));
        // dictate patch: a disallowed token loses even with the highest logit.
        assert_eq!(
            pick(&[0.1, 0.9, 0.2, 0.3, 0.7], 3, Some(&[true, false, true]), None),
            (2, 1)
        );
    }

    /// A one-node boost: `(token, bonus, new_word)` edges; tokens 0 and 1 start words.
    fn boost(edges: &[(usize, f32, bool)]) -> TokenBoost {
        TokenBoost {
            next: vec![
                edges
                    .iter()
                    .map(|&(token, bonus, new_word)| BoostEdge { token, to: 0, bonus, new_word })
                    .collect(),
            ],
            word_start: vec![true, true, false, false],
        }
    }

    #[test]
    fn boost_picks_between_tokens_but_never_beats_blank() {
        // vocabulary of 4 (3 = blank), then 2 duration logits
        let logits = [0.1, 0.9, 0.5, 0.2, 0.3, 0.7];
        let pick_with = |logits: &[f32], edges, mask: Option<&[bool]>| {
            pick(logits, 4, mask, Some((&boost(edges), 0)))
        };
        assert_eq!(pick_with(&logits, &[(2, 0.5, false)], None), (2, 1));
        assert_eq!(pick_with(&logits, &[(2, 0.3, false)], None), (1, 1));
        // the leader keeps its own bonus
        assert_eq!(pick_with(&logits, &[(1, 0.1, false), (2, 0.45, false)], None), (1, 1));
        // a masked token gets no bonus
        let mask = [true, true, false, true];
        assert_eq!(pick_with(&logits, &[(2, 0.5, false)], Some(&mask)), (1, 1));
        // blank wins: the bonus does not make a token out of it
        let blank = [0.1, 0.5, 0.4, 0.9, 0.3, 0.7];
        assert_eq!(pick_with(&blank, &[(2, 5.0, false)], None), (3, 1));
        // a new word only competes with a word start: 0 beats 1, but not 2 (inside a word)
        assert_eq!(pick_with(&logits, &[(0, 1.0, true)], None), (0, 1));
        let inside = [0.1, 0.5, 0.9, 0.2, 0.3, 0.7];
        assert_eq!(pick_with(&inside, &[(0, 1.0, true)], None), (2, 1));
        assert_eq!(pick_with(&inside, &[(0, 1.0, false)], None), (0, 1));
    }

    #[test]
    fn boost_steps_along_paths_and_falls_back_to_the_start() {
        let edge = |token, to| BoostEdge { token, to, bonus: 1.0, new_word: false };
        let boost = TokenBoost {
            next: vec![vec![edge(5, 1), edge(7, 0)], vec![edge(6, 0)]],
            word_start: Vec::new(),
        };
        assert_eq!(boost.step(0, 5), 1);
        assert_eq!(boost.step(1, 6), 0);
        // path broken: a start token still starts, anything else goes back to the start
        assert_eq!(boost.step(1, 5), 1);
        assert_eq!(boost.step(1, 9), 0);
        assert_eq!(boost.step(0, 9), 0);
    }
}
