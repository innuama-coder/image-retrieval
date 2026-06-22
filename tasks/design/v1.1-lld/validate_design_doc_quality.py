from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import sys


SECTION_GROUPS = {
    "source_traceability": ["source refs", "source reference", "traceability", "prd", "hld", "来源", "追溯", "需求"],
    "scope_boundary": ["scope", "boundary", "allowed", "forbidden", "non-goal", "范围", "边界", "非目标"],
    "control_flow": ["control flow", "sequence", "state transition", "流程", "控制流", "时序", "状态转换"],
    "data_flow": ["data flow", "transformation", "persistence flow", "数据流", "数据转换", "持久化流"],
    "interfaces_types": ["interface", "contract", "type", "dto", "api", "trait", "接口", "契约", "类型"],
    "state_persistence": ["state", "persistence", "sqlite", "transaction", "schema", "状态", "持久化", "事务"],
    "errors_diagnostics": ["error", "diagnostic", "failure", "rollback", "错误", "异常", "诊断", "失败"],
    "security_permissions": ["security", "permission", "authorization", "identity", "安全", "权限", "授权", "身份"],
    "observability": ["observability", "audit", "log", "metric", "trace", "可观测", "审计", "日志", "指标"],
    "verification": ["verification", "test", "acceptance", "fixture", "验收", "验证", "测试", "夹具"],
    "risks_handoff": ["risk", "handoff", "open question", "blocker", "风险", "交付", "移交", "阻塞"],
}

ACCEPTANCE_GROUPS = {
    "verdict": ["verdict", "pass", "fail", "blocked", "结论", "通过", "失败", "阻塞"],
    "inventory": ["document inventory", "inventory", "文档清单", "交付物清单"],
    "coverage": ["coverage", "traceability", "requirement", "覆盖", "追溯", "需求"],
    "cross_document": ["cross-document", "consistency", "dependency", "跨文档", "一致性", "依赖"],
    "quality": ["quality", "detailed design", "control flow", "data flow", "质量", "详细设计", "控制流", "数据流"],
    "blockers": ["blocker", "required fix", "risk", "阻塞", "必须修复", "风险"],
    "no_code": ["no code", "documentation-only", "code change", "文档", "不得实现", "代码变更"],
}

FORBIDDEN_TERMS = [
    "todo: fill",
    "placeholder",
    "lorem ipsum",
    "implemented files",
    "changed files",
    "implementation complete",
    "code changes:",
    "已实现文件",
    "已修改代码",
]


def fail(errors: list[str]) -> None:
    for error in errors:
        print(f"FAIL: {error}")
    sys.exit(1)


def normalize(text: str) -> str:
    return re.sub(r"\s+", " ", text.lower())


def load_json(path: Path) -> dict | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:  # noqa: BLE001
        return None


def find_planning(document: Path, explicit: str | None) -> Path | None:
    if explicit:
        return Path(explicit)
    cwd_candidate = Path("tasks/design/v1.0.0-cli-flow-design/design-planning.json")
    if cwd_candidate.exists():
        return cwd_candidate
    for parent in [document.resolve().parent, *document.resolve().parents]:
        candidate = parent / "tasks/design/v1.0.0-cli-flow-design/design-planning.json"
        if candidate.exists():
            return candidate
    return None


def safe_rel(path: Path) -> str:
    return path.as_posix().replace("\\", "/")


def section_coverage(body: str, groups: dict[str, list[str]]) -> tuple[list[str], int]:
    missing = [name for name, aliases in groups.items() if not any(alias in body for alias in aliases)]
    return missing, len(groups) - len(missing)


def keywords(value: str) -> list[str]:
    raw = re.findall(r"[A-Za-z][A-Za-z0-9_-]{2,}|[\u4e00-\u9fff]{2,}", value.lower())
    stop = {"the", "and", "for", "with", "design", "document", "specify", "review", "against"}
    seen: list[str] = []
    for item in raw:
        if item in stop or item in seen:
            continue
        seen.append(item)
    return seen


def task_for_document(plan: dict, document: Path) -> dict | None:
    doc = safe_rel(document)
    if doc.startswith("./"):
        doc = doc[2:]
    for task in plan.get("tasks", []):
        if isinstance(task, dict) and task.get("design_doc_path") == doc:
            return task
    for task in plan.get("tasks", []):
        if isinstance(task, dict) and Path(str(task.get("design_doc_path", ""))).name == document.name:
            return task
    return None


def validate_task_context(text: str, body: str, task: dict) -> list[str]:
    errors: list[str] = []
    task_id = str(task.get("task_id", ""))
    if task_id and task_id.lower() not in body:
        errors.append(f"design document must name its task id: {task_id}")

    source_refs = [str(ref) for ref in task.get("source_refs", []) if str(ref).strip()]
    visible_refs = [ref for ref in source_refs if ref.lower() in body]
    min_refs = min(3, len(source_refs))
    if len(visible_refs) < min_refs:
        errors.append(
            f"design document must cite at least {min_refs} planned source refs; visible refs: {', '.join(visible_refs) or '<none>'}"
        )

    outputs = [str(item) for item in task.get("outputs", []) if str(item).strip()]
    weak_outputs: list[str] = []
    for output in outputs:
        keys = keywords(output)
        if keys and sum(1 for key in keys if key in body) < min(2, len(keys)):
            weak_outputs.append(output)
    if weak_outputs:
        errors.append("design document does not visibly cover planned outputs: " + "; ".join(weak_outputs[:3]))

    forbidden_scope = [str(item) for item in task.get("forbidden_scope", []) if str(item).strip()]
    if forbidden_scope and "forbidden" not in body and "禁止" not in body and "不得" not in body:
        errors.append("design document must include explicit forbidden-scope or non-goal boundaries")

    if "docs/design/" not in body:
        errors.append("design document must reference docs/design/ handoff location")
    if "do not implement code" in body and "detailed design" not in body and "详细设计" not in body:
        errors.append("document reads like task instructions rather than detailed design")
    return errors


def validate_acceptance_context(body: str, task: dict | None, plan: dict | None) -> list[str]:
    errors: list[str] = []
    if not plan:
        errors.append("acceptance validation requires design-planning.json context")
        return errors
    detailed = [t for t in plan.get("tasks", []) if isinstance(t, dict) and t.get("task_type") == "detailed_design"]
    missing_tasks = [t.get("task_id") for t in detailed if str(t.get("task_id", "")).lower() not in body]
    if missing_tasks:
        errors.append("acceptance report must mention every detailed design task: " + ", ".join(missing_tasks))
    missing_docs = [
        t.get("design_doc_path")
        for t in detailed
        if str(t.get("design_doc_path", "")).lower() not in body and Path(str(t.get("design_doc_path", ""))).name.lower() not in body
    ]
    if missing_docs:
        errors.append("acceptance report must inventory every planned design doc: " + ", ".join(missing_docs[:5]))
    if task and str(task.get("design_doc_path", "")).lower() not in body:
        errors.append("acceptance report must name its own docs/design output path")
    return errors


def validate(path: Path, acceptance: bool, planning_path: str | None) -> list[str]:
    errors: list[str] = []
    if not path.exists():
        return [f"missing design document: {path}"]
    if path.suffix.lower() != ".md":
        errors.append(f"design document must be markdown: {path}")

    text = path.read_text(encoding="utf-8")
    body = normalize(text)
    min_chars = 2400 if not acceptance else 1800
    if len(text.strip()) < min_chars:
        errors.append(f"design document is too small for detailed design quality: {path}")

    headings = re.findall(r"(?m)^#{1,3}\s+\S+", text)
    min_headings = 8 if not acceptance else 6
    if len(headings) < min_headings:
        errors.append(f"design document has too few meaningful markdown headings: {path}")

    if acceptance:
        missing_groups, covered = section_coverage(body, ACCEPTANCE_GROUPS)
        if covered < len(ACCEPTANCE_GROUPS):
            errors.append(f"acceptance report missing required review dimensions: {', '.join(missing_groups)}")
    else:
        missing_groups, covered = section_coverage(body, SECTION_GROUPS)
        if covered < 10:
            errors.append(f"design document missing required detailed-design dimensions: {', '.join(missing_groups)}")

    for forbidden in FORBIDDEN_TERMS:
        if forbidden in body:
            errors.append(f"design document contains forbidden placeholder/report wording: {forbidden}")

    if "docs/" not in body and "req-" not in body and "hld" not in body:
        errors.append("design document lacks visible source traceability markers")

    plan_path = find_planning(path, planning_path)
    plan = load_json(plan_path) if plan_path and plan_path.exists() else None
    task = task_for_document(plan, path) if plan else None
    if plan and not task:
        errors.append(f"design document is not declared as a deliverable in {plan_path}")
    if task and not acceptance:
        errors.extend(validate_task_context(text, body, task))
    if acceptance:
        errors.extend(validate_acceptance_context(body, task, plan))

    return errors


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--planning")
    parser.add_argument("document")
    parser.add_argument("--acceptance", action="store_true")
    args = parser.parse_args()

    errors = validate(Path(args.document), args.acceptance, args.planning)
    if errors:
        fail(errors)
    print(f"PASS: detailed design quality validation {args.document}")


if __name__ == "__main__":
    main()
