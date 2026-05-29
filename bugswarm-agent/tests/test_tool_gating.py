"""M003: Tool capability gating — validation middleware tests."""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from agent.tools import (
    MAX_CALLS_PER_WINDOW,
    MAX_CONTENT_BYTES,
    MAX_COUNT,
    MAX_HOPS,
    MAX_INPUT_SIZE_BYTES,
    MAX_POC_SIZE_BYTES,
    MAX_QUERIES,
    MAX_SOURCE_SIZE_BYTES,
    RATE_LIMIT_WINDOW_SECS,
    TOOL_VALIDATORS,
    _validate_arg_types,
    _validate_delta_debug,
    _validate_describe_trigger,
    _validate_diff_execute,
    _validate_edit_file,
    _validate_exec_sandbox,
    _validate_explore_paths,
    _validate_fuzz_target,
    _validate_get_trigger_matrix,
    _validate_glob,
    _validate_grep,
    _validate_kill_shell,
    _validate_list_dir,
    _validate_mine_invariants,
    _validate_predict_fix_impact,
    _validate_query_cpg,
    _validate_read_file,
    _validate_run_mutations,
    _validate_solve_reachability,
    _validate_suggest_chain,
    _validate_todo_write,
    _validate_trace_dependency,
    _validate_web_fetch,
    _validate_write_file,
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
        ok, reason = _validate_diff_execute({"input": "", "reference": ""})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_diff_execute({"input": "hello", "reference": "world"})
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
            {"bug_id": "", "function_name": "f", "original_line": "x", "replacement_line": "y"}
        )
        assert not ok

    def test_empty_function_rejected(self):
        ok, reason = _validate_predict_fix_impact(
            {"bug_id": "B1", "function_name": "", "original_line": "x", "replacement_line": "y"}
        )
        assert not ok

    def test_empty_original_rejected(self):
        ok, reason = _validate_predict_fix_impact(
            {"bug_id": "B1", "function_name": "f", "original_line": "", "replacement_line": "y"}
        )
        assert not ok

    def test_empty_replacement_rejected(self):
        ok, reason = _validate_predict_fix_impact(
            {"bug_id": "B1", "function_name": "f", "original_line": "x", "replacement_line": ""}
        )
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_predict_fix_impact(
            {"bug_id": "B1", "function_name": "f", "original_line": "x", "replacement_line": "y"}
        )
        assert ok


class TestValidatorRegistry:
    def test_all_23_tools_have_validators(self):
        expected = {
            "exec_sandbox",
            "read_file",
            "fuzz_target",
            "delta_debug",
            "diff_execute",
            "mine_invariants",
            "run_mutations",
            "solve_reachability",
            "explore_paths",
            "describe_trigger",
            "get_trigger_matrix",
            "suggest_chain",
            "predict_fix_impact",
            "query_cpg",
            "list_dir",
            "trace_dependency",
            "grep",
            "glob",
            "write_file",
            "edit_file",
            "web_fetch",
            "todo_write",
            "kill_shell",
        }
        assert set(TOOL_VALIDATORS.keys()) == expected, f"Missing validators: {expected - set(TOOL_VALIDATORS.keys())}"

    def test_every_validator_returns_tuple(self):
        for name, validator in TOOL_VALIDATORS.items():
            minimal = {
                "path": "test.py",
                "poc_code": "x",
                "target_path": "bin",
                "function_name": "f",
                "bug_id": "B1",
                "bug_ids": ["B1"],
                "source_code": "x",
                "target_location": "f:1",
                "output_a": "x",
                "output_b": "y",
                "original_line": "x",
                "replacement_line": "y",
                "input_base64": "AA",
                "max_iterations": 200,
                "max_queries": 100,
                "max_hops": 10,
                "count": 100,
                "dimension": "Input",
                "exec_timeout_ms": 1000,
                "name": "f",
                "kind": "function",
                "pattern": "test",
                "max_results": 100,
                "content": "x",
                "old_string": "a",
                "new_string": "b",
                "replace_all": False,
                "url": "https://example.com",
                "action": "list",
                "status": "pending",
                "priority": 5,
                "signal": "SIGTERM",
                "kill_all": False,
                "process_id": None,
            }
            ok, reason = validator(minimal)
            assert isinstance(ok, bool), f"{name} validator returned non-bool"
            assert isinstance(reason, str), f"{name} validator returned non-str reason"


class TestQueryCpgValidation:
    def test_valid_empty_accepted(self):
        ok, reason = _validate_query_cpg({})
        assert ok

    def test_invalid_name_type_rejected(self):
        ok, reason = _validate_query_cpg({"name": 123})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_query_cpg({"name": "login", "kind": "function"})
        assert ok


class TestListDirValidation:
    def test_empty_path_rejected(self):
        ok, reason = _validate_list_dir({"path": ""})
        assert not ok

    def test_null_byte_rejected(self):
        ok, reason = _validate_list_dir({"path": "dir\0file"})
        assert not ok

    def test_absolute_path_rejected(self):
        ok, reason = _validate_list_dir({"path": "/etc"})
        assert not ok

    def test_traversal_rejected(self):
        ok, reason = _validate_list_dir({"path": "../etc"})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_list_dir({"path": "subdir"})
        assert ok

    def test_default_path_accepted(self):
        ok, reason = _validate_list_dir({})
        assert ok


class TestTraceDependencyValidation:
    def test_empty_function_rejected(self):
        ok, reason = _validate_trace_dependency({"function_name": ""})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_trace_dependency({"function_name": "login", "radius": 5})
        assert ok


class TestGrepValidation:
    def test_empty_pattern_rejected(self):
        ok, reason = _validate_grep({"pattern": ""})
        assert not ok

    def test_max_results_too_high_rejected(self):
        ok, reason = _validate_grep({"pattern": "test", "max_results": 5001})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_grep({"pattern": "import os", "max_results": 100})
        assert ok


class TestGlobValidation:
    def test_max_results_too_high_rejected(self):
        ok, reason = _validate_glob({"max_results": 10001})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_glob({"pattern": "**/*.py", "max_results": 50})
        assert ok

    def test_empty_args_accepted(self):
        ok, reason = _validate_glob({})
        assert ok


class TestWriteFileValidation:
    def test_empty_path_rejected(self):
        ok, reason = _validate_write_file({"path": "", "content": "x"})
        assert not ok

    def test_null_byte_rejected(self):
        ok, reason = _validate_write_file({"path": "f\0ile", "content": "x"})
        assert not ok

    def test_absolute_path_rejected(self):
        ok, reason = _validate_write_file({"path": "/etc/passwd", "content": "x"})
        assert not ok

    def test_traversal_rejected(self):
        ok, reason = _validate_write_file({"path": "../etc/passwd", "content": "x"})
        assert not ok

    def test_empty_content_rejected(self):
        ok, reason = _validate_write_file({"path": "test.py", "content": ""})
        assert not ok

    def test_oversized_content_rejected(self):
        ok, reason = _validate_write_file({"path": "test.py", "content": "x" * (MAX_CONTENT_BYTES + 1)})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_write_file({"path": "src/main.py", "content": "print(1)"})
        assert ok


class TestEditFileValidation:
    def test_empty_path_rejected(self):
        ok, reason = _validate_edit_file({"path": "", "old_string": "a", "new_string": "b"})
        assert not ok

    def test_null_byte_rejected(self):
        ok, reason = _validate_edit_file({"path": "f\0ile", "old_string": "a", "new_string": "b"})
        assert not ok

    def test_absolute_path_rejected(self):
        ok, reason = _validate_edit_file({"path": "/etc/passwd", "old_string": "a", "new_string": "b"})
        assert not ok

    def test_traversal_rejected(self):
        ok, reason = _validate_edit_file({"path": "../etc/passwd", "old_string": "a", "new_string": "b"})
        assert not ok

    def test_empty_old_string_rejected(self):
        ok, reason = _validate_edit_file({"path": "f.py", "old_string": "", "new_string": "b"})
        assert not ok

    def test_empty_new_string_rejected(self):
        ok, reason = _validate_edit_file({"path": "f.py", "old_string": "a", "new_string": ""})
        assert not ok

    def test_invalid_replace_all_type_rejected(self):
        ok, reason = _validate_edit_file({"path": "f.py", "old_string": "a", "new_string": "b", "replace_all": "yes"})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_edit_file({"path": "f.py", "old_string": "a", "new_string": "b"})
        assert ok


class TestWebFetchValidation:
    def test_empty_url_rejected(self):
        ok, reason = _validate_web_fetch({"url": ""})
        assert not ok

    def test_invalid_scheme_rejected(self):
        ok, reason = _validate_web_fetch({"url": "ftp://example.com"})
        assert not ok

    def test_valid_accepted(self):
        ok, reason = _validate_web_fetch({"url": "https://github.com"})
        assert ok


class TestTodoWriteValidation:
    def test_empty_action_rejected(self):
        ok, reason = _validate_todo_write({"action": ""})
        assert not ok

    def test_invalid_action_rejected(self):
        ok, reason = _validate_todo_write({"action": "delete"})
        assert not ok

    def test_invalid_status_rejected(self):
        ok, reason = _validate_todo_write({"action": "add", "status": "cancelled"})
        assert not ok

    def test_priority_out_of_range_rejected(self):
        ok, reason = _validate_todo_write({"action": "add", "priority": 11})
        assert not ok

    def test_valid_add_accepted(self):
        ok, reason = _validate_todo_write({"action": "add", "description": "Fix bug", "priority": 5})
        assert ok

    def test_valid_list_accepted(self):
        ok, reason = _validate_todo_write({"action": "list"})
        assert ok


class TestKillShellValidation:
    def test_invalid_signal_rejected(self):
        ok, reason = _validate_kill_shell({"signal": "SIGHUP"})
        assert not ok

    def test_invalid_kill_all_type_rejected(self):
        ok, reason = _validate_kill_shell({"kill_all": "yes"})
        assert not ok

    def test_valid_sigterm_accepted(self):
        ok, reason = _validate_kill_shell({"signal": "SIGTERM"})
        assert ok

    def test_valid_sigkill_accepted(self):
        ok, reason = _validate_kill_shell({"signal": "SIGKILL"})
        assert ok

    def test_default_signal_accepted(self):
        ok, reason = _validate_kill_shell({})
        assert ok


class TestArgTypesValidation:
    def test_integer_arg_rejects_string(self):
        ok, reason = _validate_arg_types(
            "test", {"count": "not_an_int"}, {"properties": {"count": {"type": "integer"}}}
        )
        assert not ok

    def test_boolean_arg_rejects_string(self):
        ok, reason = _validate_arg_types("test", {"flag": "yes"}, {"properties": {"flag": {"type": "boolean"}}})
        assert not ok

    def test_string_arg_rejects_int(self):
        ok, reason = _validate_arg_types("test", {"name": 42}, {"properties": {"name": {"type": "string"}}})
        assert not ok

    def test_array_arg_rejects_string(self):
        ok, reason = _validate_arg_types("test", {"items": "not_array"}, {"properties": {"items": {"type": "array"}}})
        assert not ok

    def test_valid_types_accepted(self):
        ok, reason = _validate_arg_types(
            "test",
            {"count": 5, "flag": True, "name": "x"},
            {"properties": {"count": {"type": "integer"}, "flag": {"type": "boolean"}, "name": {"type": "string"}}},
        )
        assert ok

    def test_unknown_arg_ignored(self):
        ok, reason = _validate_arg_types("test", {"unknown": "whatever"}, {"properties": {}})
        assert ok


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
