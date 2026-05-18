"""M003: Tool capability gating — validation middleware tests."""

import pytest
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from agent.tools import (
    _validate_exec_sandbox, _validate_read_file, _validate_fuzz_target,
    _validate_delta_debug, _validate_diff_execute, _validate_mine_invariants,
    _validate_run_mutations, _validate_solve_reachability, _validate_explore_paths,
    _validate_describe_trigger, _validate_get_trigger_matrix, _validate_suggest_chain,
    _validate_predict_fix_impact,
    TOOL_VALIDATORS, MAX_POC_SIZE_BYTES, MAX_INPUT_SIZE_BYTES, MAX_SOURCE_SIZE_BYTES,
    MAX_COUNT, MAX_QUERIES, MAX_HOPS,
)


class TestExecSandboxValidation:
    def test_empty_poc_rejected(self):
        ok, reason = _validate_exec_sandbox({"poc_code": ""})
        assert not ok
        assert "non-empty" in reason

    def test_oversized_poc_rejected(self):
        ok, reason = _validate_exec_sandbox({"poc_code": "x" * (MAX_POC_SIZE_BYTES + 1)})
        assert not ok
        assert "too large" in reason

    def test_valid_poc_accepted(self):
        ok, reason = _validate_exec_sandbox({"poc_code": "print('hello')"})
        assert ok


class TestReadFileValidation:
    def test_absolute_path_rejected(self):
        ok, reason = _validate_read_file({"path": "/etc/passwd"})
        assert not ok
        assert "absolute" in reason.lower()

    def test_parent_traversal_rejected(self):
        ok, reason = _validate_read_file({"path": "../etc/passwd"})
        assert not ok
        assert "traversal" in reason.lower()

    def test_subdir_traversal_rejected(self):
        ok, reason = _validate_read_file({"path": "subdir/../../etc/passwd"})
        assert not ok
        assert "traversal" in reason.lower()

    def test_valid_path_accepted(self):
        ok, reason = _validate_read_file({"path": "auth.py"})
        assert ok

    def test_empty_path_rejected(self):
        ok, reason = _validate_read_file({"path": ""})
        assert not ok


class TestFuzzTargetValidation:
    def test_empty_target_rejected(self):
        ok, reason = _validate_fuzz_target({"target_path": ""})
        assert not ok

    def test_timeout_too_low_rejected(self):
        ok, reason = _validate_fuzz_target({"target_path": "bin", "exec_timeout_ms": 5})
        assert not ok

    def test_timeout_too_high_rejected(self):
        ok, reason = _validate_fuzz_target({"target_path": "bin", "exec_timeout_ms": 500_000})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_fuzz_target({"target_path": "/bin/true", "exec_timeout_ms": 1000})
        assert ok


class TestDeltaDebugValidation:
    def test_too_many_iterations_rejected(self):
        ok, reason = _validate_delta_debug({"input_base64": "AAAA", "max_iterations": MAX_COUNT + 1})
        assert not ok

    def test_oversized_input_rejected(self):
        large = "x" * (MAX_INPUT_SIZE_BYTES + 1)
        ok, reason = _validate_delta_debug({"input_bytes": large.encode()})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_delta_debug({"input_base64": "AAAA", "max_iterations": 100})
        assert ok


class TestDiffExecuteValidation:
    def test_empty_outputs_rejected(self):
        ok, reason = _validate_diff_execute({"output_a": "", "output_b": ""})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_diff_execute({"output_a": "hello", "output_b": "world"})
        assert ok


class TestMineInvariantsValidation:
    def test_empty_function_rejected(self):
        ok, reason = _validate_mine_invariants({"function_name": ""})
        assert not ok

    def test_count_too_high_rejected(self):
        ok, reason = _validate_mine_invariants({"function_name": "f", "count": MAX_COUNT + 1})
        assert not ok

    def test_count_too_low_rejected(self):
        ok, reason = _validate_mine_invariants({"function_name": "f", "count": 0})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_mine_invariants({"function_name": "func", "count": 100})
        assert ok


class TestRunMutationsValidation:
    def test_empty_source_rejected(self):
        ok, reason = _validate_run_mutations({"source_code": ""})
        assert not ok

    def test_oversized_source_rejected(self):
        ok, reason = _validate_run_mutations({"source_code": "x" * (MAX_SOURCE_SIZE_BYTES + 1)})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_run_mutations({"source_code": "def f(): pass"})
        assert ok


class TestSolveReachabilityValidation:
    def test_empty_target_rejected(self):
        ok, reason = _validate_solve_reachability({"target_location": ""})
        assert not ok

    def test_invalid_format_rejected(self):
        ok, reason = _validate_solve_reachability({"target_location": "just_a_filename"})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_solve_reachability({"target_location": "auth.py:42"})
        assert ok


class TestExplorePathsValidation:
    def test_empty_target_rejected(self):
        ok, reason = _validate_explore_paths({"target_location": ""})
        assert not ok

    def test_queries_too_high_rejected(self):
        ok, reason = _validate_explore_paths({"target_location": "a.py:1", "max_queries": MAX_QUERIES + 1})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_explore_paths({"target_location": "a.py:1", "max_queries": 50})
        assert ok


class TestDescribeTriggerValidation:
    def test_empty_bug_id_rejected(self):
        ok, reason = _validate_describe_trigger({"bug_id": ""})
        assert not ok

    def test_invalid_dimension_rejected(self):
        ok, reason = _validate_describe_trigger({"bug_id": "B1", "dimension": "NotReal"})
        assert not ok

    def test_valid_dimension_accepted(self):
        ok, reason = _validate_describe_trigger({"bug_id": "B1", "dimension": "Input"})
        assert ok


class TestGetTriggerMatrixValidation:
    def test_empty_bug_id_rejected(self):
        ok, reason = _validate_get_trigger_matrix({"bug_id": ""})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_get_trigger_matrix({"bug_id": "B1"})
        assert ok


class TestSuggestChainValidation:
    def test_empty_ids_rejected(self):
        ok, reason = _validate_suggest_chain({"bug_ids": []})
        assert not ok

    def test_too_many_ids_rejected(self):
        ok, reason = _validate_suggest_chain({"bug_ids": ["B" + str(i) for i in range(101)]})
        assert not ok

    def test_hops_too_high_rejected(self):
        ok, reason = _validate_suggest_chain({"bug_ids": ["B1"], "max_hops": MAX_HOPS + 1})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_suggest_chain({"bug_ids": ["B1", "B2"], "max_hops": 5})
        assert ok


class TestPredictFixImpactValidation:
    def test_empty_bug_id_rejected(self):
        ok, reason = _validate_predict_fix_impact(
            {"bug_id": "", "function_name": "f", "original_line": "x", "replacement_line": "y"})
        assert not ok

    def test_empty_function_rejected(self):
        ok, reason = _validate_predict_fix_impact(
            {"bug_id": "B1", "function_name": "", "original_line": "x", "replacement_line": "y"})
        assert not ok

    def test_empty_original_rejected(self):
        ok, reason = _validate_predict_fix_impact(
            {"bug_id": "B1", "function_name": "f", "original_line": "", "replacement_line": "y"})
        assert not ok

    def test_empty_replacement_rejected(self):
        ok, reason = _validate_predict_fix_impact(
            {"bug_id": "B1", "function_name": "f", "original_line": "x", "replacement_line": ""})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_predict_fix_impact(
            {"bug_id": "B1", "function_name": "f", "original_line": "x", "replacement_line": "y"})
        assert ok


class TestValidatorRegistry:
    def test_all_13_tools_have_validators(self):
        expected = {
            "exec_sandbox", "read_file", "fuzz_target", "delta_debug",
            "diff_execute", "mine_invariants", "run_mutations",
            "solve_reachability", "explore_paths", "describe_trigger",
            "get_trigger_matrix", "suggest_chain", "predict_fix_impact",
        }
        assert set(TOOL_VALIDATORS.keys()) == expected, \
            f"Missing validators: {expected - set(TOOL_VALIDATORS.keys())}"

    def test_every_validator_returns_tuple(self):
        for name, validator in TOOL_VALIDATORS.items():
            # Test with minimal valid args
            minimal = {"path": "test.py", "poc_code": "x", "target_path": "bin",
                       "function_name": "f", "bug_id": "B1", "bug_ids": ["B1"],
                       "source_code": "x", "target_location": "f:1",
                       "output_a": "x", "output_b": "y",
                       "original_line": "x", "replacement_line": "y",
                       "input_base64": "AA", "max_iterations": 200,
                       "max_queries": 100, "max_hops": 10, "count": 100,
                       "dimension": "Input", "exec_timeout_ms": 1000}
            ok, reason = validator(minimal)
            assert isinstance(ok, bool), f"{name} validator returned non-bool"
            assert isinstance(reason, str), f"{name} validator returned non-str reason"


class TestRateLimitConstants:
    def test_constants_are_reasonable(self):
        assert MAX_POC_SIZE_BYTES == 1_000_000
        assert MAX_INPUT_SIZE_BYTES == 10_000_000
        assert MAX_SOURCE_SIZE_BYTES == 5_000_000
        assert MAX_COUNT == 10_000
        assert MAX_QUERIES == 1_000
        assert MAX_HOPS == 20
        assert RATE_LIMIT_WINDOW_SECS == 60
        assert MAX_CALLS_PER_WINDOW == 100


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
