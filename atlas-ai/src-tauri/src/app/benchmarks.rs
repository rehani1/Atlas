use rusqlite::Connection;

use crate::{
    domain::benchmark::ModelBenchmark,
    infra::benchmarks::{self, CompletedBenchmarkMetrics},
};

pub(crate) struct BenchmarkPrompt {
    pub(crate) prompt_type: &'static str,
    pub(crate) prompt_label: &'static str,
    pub(crate) prompt_text: &'static str,
}

pub(crate) const BENCHMARK_SUITE: &[BenchmarkPrompt] = &[
    BenchmarkPrompt {
        prompt_type: "short_factual",
        prompt_label: "Short factual",
        prompt_text: "Answer in one concise sentence: What is SQLite WAL mode?",
    },
    BenchmarkPrompt {
        prompt_type: "long_explanation",
        prompt_label: "Long explanation",
        prompt_text: "Explain in four short paragraphs how local-first desktop AI apps can protect user data while still feeling responsive.",
    },
    BenchmarkPrompt {
        prompt_type: "code_explanation",
        prompt_label: "Code explanation",
        prompt_text: "Explain what this Rust function does and mention one edge case:\n\nfn clamp(value: i32, min: i32, max: i32) -> i32 {\n    value.max(min).min(max)\n}",
    },
    BenchmarkPrompt {
        prompt_type: "json_output",
        prompt_label: "JSON output",
        prompt_text: "Return only valid JSON with keys summary, risks, and next_steps for a local AI chat app benchmark.",
    },
    BenchmarkPrompt {
        prompt_type: "summarization",
        prompt_label: "Summarization",
        prompt_text: "Summarize this release note in three bullets: Atlas added local chat history, cancellable model downloads, generation diagnostics, SQLite FTS search, and database health reporting.",
    },
];

pub(crate) fn create_suite(
    conn: &Connection,
    job_id: &str,
    model_name: &str,
    now: i64,
) -> Result<Vec<ModelBenchmark>, String> {
    benchmarks::insert_suite(
        conn,
        job_id,
        model_name,
        BENCHMARK_SUITE
            .iter()
            .map(|prompt| {
                (
                    prompt.prompt_type,
                    prompt.prompt_label,
                    prompt_text_hash(prompt.prompt_text),
                )
            })
            .collect::<Vec<_>>()
            .as_slice(),
        now,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn list_recent(conn: &Connection, limit: i64) -> Result<Vec<ModelBenchmark>, String> {
    benchmarks::list_recent(conn, limit).map_err(|error| error.to_string())
}

pub(crate) fn mark_running(
    conn: &Connection,
    benchmark_id: &str,
    started_at: i64,
) -> Result<ModelBenchmark, String> {
    benchmarks::mark_running(conn, benchmark_id, started_at).map_err(|error| error.to_string())
}

pub(crate) fn mark_completed(
    conn: &Connection,
    benchmark_id: &str,
    completed_at: i64,
    metrics: &CompletedBenchmarkMetrics,
) -> Result<ModelBenchmark, String> {
    benchmarks::mark_completed(conn, benchmark_id, completed_at, metrics)
        .map_err(|error| error.to_string())
}

pub(crate) fn mark_cancelled(
    conn: &Connection,
    benchmark_id: &str,
    completed_at: i64,
) -> Result<ModelBenchmark, String> {
    benchmarks::mark_cancelled(conn, benchmark_id, completed_at).map_err(|error| error.to_string())
}

pub(crate) fn mark_failed(
    conn: &Connection,
    benchmark_id: &str,
    completed_at: i64,
    error_message: &str,
) -> Result<ModelBenchmark, String> {
    benchmarks::mark_failed(conn, benchmark_id, completed_at, error_message)
        .map_err(|error| error.to_string())
}

pub(crate) fn mark_remaining_cancelled(
    conn: &Connection,
    job_id: &str,
    completed_at: i64,
) -> Result<usize, String> {
    benchmarks::mark_remaining_cancelled(conn, job_id, completed_at)
        .map_err(|error| error.to_string())
}

pub(crate) fn mark_remaining_failed(
    conn: &Connection,
    job_id: &str,
    completed_at: i64,
    error_message: &str,
) -> Result<usize, String> {
    benchmarks::mark_remaining_failed(conn, job_id, completed_at, error_message)
        .map_err(|error| error.to_string())
}

fn prompt_text_hash(prompt_text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;

    for byte in prompt_text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::prompt_text_hash;

    #[test]
    fn prompt_text_hash_is_stable() {
        assert_eq!(
            prompt_text_hash("benchmark prompt"),
            prompt_text_hash("benchmark prompt")
        );
        assert_ne!(
            prompt_text_hash("benchmark prompt"),
            prompt_text_hash("different prompt")
        );
    }
}
