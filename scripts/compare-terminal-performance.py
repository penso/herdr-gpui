#!/usr/bin/env python3
"""Compare an already-built native release harness; never builds anything.

Usage: python3 -B scripts/compare-terminal-performance.py --pairs 5
       python3 -B scripts/compare-terminal-performance.py --output comparison.json

Default reference: previous per-cell algorithm in the SAME binary, NOT an actual
historical executable. --baseline-binary permits an existing compatible historical
harness (same flags, environment switches, and JSON schema). Run on a quiet native
desktop. Only an explicitly requested --output file is written by this script.
"""

import argparse
import datetime
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys


def percentile(samples, percent):
    """Nearest rank, matching the native harness (not interpolated)."""
    return sorted(samples)[max(0, math.ceil(len(samples) * percent / 100) - 1)]


def parse_samples(stdout):
    samples = {}
    results = []
    for line in stdout.splitlines():
        if not line.lstrip().startswith("{"):
            continue
        record = json.loads(line)
        if "result" in record:
            results.append(record["result"])
        if "category" not in record:
            continue
        category = record["category"]
        values = record.get("samples_ms")
        if not isinstance(category, str) or not category or category in samples:
            raise ValueError("invalid or duplicate sample category")
        if not isinstance(values, list) or not values or any(
            type(value) not in (int, float) or not math.isfinite(value) or value < 0
            for value in values
        ):
            raise ValueError(f"invalid samples for {category}")
        samples[category] = values
    if results != ["PASS"] or not samples:
        raise ValueError("expected samples and exactly one JSON result PASS")
    return samples


def compare(runs, max_regression, min_improvement):
    categories = set(runs[0]["samples"])
    if any(set(run["samples"]) != categories for run in runs):
        raise ValueError("category sets differ between runs/binaries")
    if not {"warm-hover", "single-cell"} <= categories:
        raise ValueError("missing required warm-hover or single-cell samples")
    rows, failures = {}, []
    for category in sorted(categories):
        counts = [len(run["samples"][category]) for run in runs]
        row = {"sample_counts": counts, "regression_gated": min(counts) >= 12}
        for percent in (50, 95):
            medians = {
                role: statistics.median(
                    percentile(run["samples"][category], percent)
                    for run in runs if run["role"] == role
                )
                for role in ("reference", "candidate")
            }
            reference, candidate = medians["reference"], medians["candidate"]
            change = (candidate / reference - 1) * 100 if reference else None
            row[f"p{percent}"] = {**medians, "change_percent": change}
            if row["regression_gated"] and candidate > reference * (1 + max_regression / 100):
                failures.append(f"{category} p{percent}: exceeds {max_regression:g}% regression")
            if percent == 95 and category in ("warm-hover", "single-cell"):
                if reference == 0 or candidate > reference * (1 - min_improvement / 100):
                    failures.append(f"{category} p95: cannot establish {min_improvement:g}% improvement")
        rows[category] = row
    return rows, failures


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, default=Path("target/release/herdr-gpui"), help="already-built candidate release binary")
    parser.add_argument("--baseline-binary", type=Path, help="existing compatible historical harness; otherwise use --binary")
    parser.add_argument("--pairs", type=int, default=5, help="interleaved pairs, alternating order (default: 5)")
    parser.add_argument("--max-regression", type=float, default=15, help="maximum p50/p95 regression percent for >=12 samples/run (default: 15)")
    parser.add_argument("--min-improvement", type=float, default=20, help="required warm-hover AND single-cell p95 improvement percent (default: 20)")
    parser.add_argument("--output", type=Path, help="write full samples, logs, comparison, and provenance as JSON")
    args = parser.parse_args()
    if args.pairs < 1:
        parser.error("--pairs must be positive")
    if not math.isfinite(args.max_regression) or args.max_regression < 0:
        parser.error("--max-regression must be finite and nonnegative")
    if not math.isfinite(args.min_improvement) or not 0 <= args.min_improvement <= 100:
        parser.error("--min-improvement must be between 0 and 100")

    binaries = {"candidate": args.binary.resolve(), "reference": (args.baseline_binary or args.binary).resolve()}
    label = (
        "existing baseline harness with previous per-cell algorithm switches"
        if args.baseline_binary else
        "previous per-cell algorithm in the SAME binary, NOT an actual historical executable"
    )
    environment = {key: value for key, value in os.environ.items() if not key.startswith("HERDR_PERF_")}
    switches = {"HERDR_PERF_RETAINED": "1", "HERDR_PERF_SAMPLES": "1", "HERDR_PERF_P95_MS": "inf"}
    overrides = {
        "candidate": dict(switches),
        "reference": {**switches, "HERDR_PERF_NO_RETAIN": "1", "HERDR_PERF_NO_BATCH": "1"},
    }
    artifact = {
        "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "platform": platform.platform(), "machine": platform.machine(),
        "python": platform.python_version(), "reference_label": label,
        "settings": {"pairs": args.pairs, "max_regression_percent": args.max_regression,
                     "min_improvement_percent": args.min_improvement, "timeout_seconds": 180},
        "environment_overrides": overrides, "binaries": {}, "runs": [],
    }
    exit_code = 2
    try:
        for role, binary in binaries.items():
            if not binary.is_file() or not os.access(binary, os.X_OK):
                raise ValueError(f"not an executable file: {binary}")
            if args.output and args.output.resolve() == binary:
                raise ValueError("--output must not overwrite a benchmark executable")
            digest = hashlib.sha256()
            with binary.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
            artifact["binaries"][role] = {"path": str(binary), "sha256": digest.hexdigest()}
        if args.output:
            repo = Path(__file__).resolve().parent.parent
            artifact["source_checkout"] = {"path": str(repo), "note": "checkout metadata, not proof of either executable's build revision"}
            for key, command in (("revision", ["git", "rev-parse", "HEAD"]),
                                 ("status", ["git", "status", "--porcelain", "--untracked-files=normal"])):
                result = subprocess.run(command, cwd=repo, capture_output=True, text=True, timeout=180,
                                        env={**os.environ, "GIT_OPTIONAL_LOCKS": "0"})
                if result.returncode:
                    raise ValueError(f"metadata command failed: {command}\n{result.stdout}\n{result.stderr}")
                artifact["source_checkout"][key] = result.stdout.strip()
            artifact["source_checkout"]["dirty"] = bool(artifact["source_checkout"]["status"])
        print(f"Reference: {label}")
        print("Candidate: retained terminal + default batching. Lower timings are better.")
        for pair in range(args.pairs):
            order = ("reference", "candidate") if pair % 2 == 0 else ("candidate", "reference")
            for role in order:
                print(f"Pair {pair + 1}/{args.pairs}: {role}", flush=True)
                run = {"pair": pair + 1, "role": role, "command": [str(binaries[role]), "--performance-test"]}
                artifact["runs"].append(run)
                try:
                    result = subprocess.run(run["command"], env={**environment, **overrides[role]},
                                            capture_output=True, text=True, errors="replace", timeout=180)
                except subprocess.TimeoutExpired as error:
                    for key in ("stdout", "stderr"):
                        value = getattr(error, key) or ""
                        run[key] = value.decode(errors="replace") if isinstance(value, bytes) else value
                    raise ValueError(f"pair {pair + 1} {role}: timed out after 180 seconds") from error
                run.update(returncode=result.returncode, stdout=result.stdout, stderr=result.stderr)
                if result.returncode:
                    raise ValueError(f"pair {pair + 1} {role}: harness exited {result.returncode}")
                run["samples"] = parse_samples(result.stdout)
        rows, failures = compare(artifact["runs"], args.max_regression, args.min_improvement)
        artifact.update(comparison=rows, failures=failures, result="TIMING FAIL" if failures else "PASS")
        print("\nMedian per-run nearest-rank percentiles (ms); delta = candidate / reference - 1")
        print(f"{'Category':<20} {'p50 ref':>10} {'p50 cand':>10} {'delta':>10} {'p95 ref':>10} {'p95 cand':>10} {'delta':>10}  Gate")
        for category, row in rows.items():
            values = []
            for key in ("p50", "p95"):
                metric = row[key]
                delta = metric["change_percent"]
                values.append(f"{metric['reference']:10.4f} {metric['candidate']:10.4f} {f'{delta:+.1f}%' if delta is not None else 'n/a (zero)':>10}")
            print(f"{category:<20} {' '.join(values)}  {'>=12 samples' if row['regression_gated'] else 'report only (<12)'}")
        if failures:
            print("\nTIMING FAIL: all harnesses passed correctness checks, but comparison thresholds failed.", file=sys.stderr)
            print("\n".join(failures), file=sys.stderr)
            print("Timing noise may contribute; this is not proof of a code regression. Repeat on a quiet desktop with more --pairs; no retries or outliers were discarded.", file=sys.stderr)
            exit_code = 1
        else:
            print("\nPASS: correctness markers and timing gates passed.")
            exit_code = 0
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        artifact.update(result="ERROR", error=str(error))
        print(f"ERROR: {error}", file=sys.stderr)
    if exit_code:
        for run in artifact["runs"]:
            print(f"\n--- pair {run['pair']} {run['role']} logs ---\nstdout:\n{run.get('stdout', '')}\nstderr:\n{run.get('stderr', '')}", file=sys.stderr)
    if args.output and args.output.resolve() not in binaries.values():
        try:
            args.output.write_text(json.dumps(artifact, indent=2, allow_nan=False) + "\n", encoding="utf-8")
        except OSError as error:
            print(f"ERROR writing artifact: {error}", file=sys.stderr)
            return 2
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
