#!/usr/bin/env python3
"""Generate #237's stratified development and sealed exam fixtures.

All identity-bearing components live in eval-bed-spec.json. This writer
never assigns an ID from an output ordinal, a row number or merchant
text. Every value is synthetic.

    python3 packs/app.kttl.subscription-audit/fixtures/make-eval-bed.py
    python3 packs/app.kttl.subscription-audit/fixtures/make-eval-bed.py --check
"""

from __future__ import annotations

import argparse
import csv
import io
import json
from datetime import date, timedelta
from pathlib import Path

HERE = Path(__file__).parent
SPEC_PATH = HERE / "eval-bed-spec.json"
GENERATED_PREFIX = "generated-"
PROCESSORS = ("stripe", "square", "paypal")


def month_day(offset: int, day: int = 8, start_year: int = 2024) -> date:
    month_index = offset
    return date(start_year + month_index // 12, month_index % 12 + 1, day)


def display_token(token: str) -> str:
    return token.replace("-", " ").title()


def merchant_name(token: str, noun: str) -> str:
    return f"{display_token(token)} {noun}"


def descriptor(base: str, merchant: str) -> str:
    full = merchant.upper()
    if base == "clean":
        return full
    if base == "messy-merchant-strings":
        compact = "".join(
            character
            for character in full
            if character == " " or character not in "AEIOU"
        )
        return f"CARD 4821 {compact[:30]} GB"
    if base == "ambiguous-categories":
        return f"{full} PAYMENT"
    raise ValueError(f"unknown base stratum: {base}")


def processor_descriptors(base: str, merchant: str) -> list[tuple[str, str]]:
    compact = merchant.upper().replace(" ", "")
    qualifier = " CARD 4821" if base == "messy-merchant-strings" else ""
    return [
        ("stripe", f"STRIPE* {compact}{qualifier}"),
        ("square", f"SQ *{compact}{qualifier}"),
        ("paypal", f"PAYPAL *{compact}{qualifier}"),
    ]


def transaction_shape(shape: str, raw_count: int) -> tuple[list[tuple[date, int, str]], str | None]:
    """Return date, descriptor index and signed amount; plus expected cadence."""

    multi = raw_count > 1
    rows: list[tuple[date, int, str]]
    period: str | None = None

    if shape in {"annual", "annual_multi"}:
        rows = [(date(2024, 3, 14), 0, "-84.00"), (date(2025, 3, 14), 1 if multi else 0, "-84.00")]
        period = None if multi else "yearly"
    elif shape in {"trial", "trial_refund"}:
        rows = [(month_day(index), 0, "-9.50") for index in range(6)]
        if shape == "trial_refund":
            rows.append((month_day(3, 12), 0, "9.50"))
        period = "monthly"
    elif shape in {"cancelled", "cancel_price"}:
        rows = [
            (
                month_day(index),
                0,
                "-13.00" if shape == "cancel_price" and index >= 4 else "-11.00",
            )
            for index in range(8)
        ]
        later_amount = "-13.00" if shape == "cancel_price" else "-11.00"
        rows.extend((month_day(13 + index), 0, later_amount) for index in range(4))
        period = "monthly"
    elif shape in {"price_multi", "energy_price"}:
        rows = [
            (
                month_day(index),
                index % raw_count,
                f"-7.{index + 10:02d}" if multi else "-7.00",
            )
            for index in range(8)
        ]
        rows.extend(
            (
                month_day(8 + index),
                (8 + index) % raw_count,
                f"-9.{index + 20:02d}" if multi else "-9.00",
            )
            for index in range(4)
        )
        period = None if multi else "monthly"
    elif shape == "monthly_refund":
        rows = [(month_day(index), 0, "-6.50") for index in range(11)]
        rows.append((month_day(10, 12), 0, "6.50"))
        period = "monthly"
    elif shape == "rent_price_multi":
        rows = [
            (month_day(index, 1), index % raw_count, f"-825.{index + 10:02d}")
            for index in range(8)
        ]
        rows.extend(
            (month_day(8 + index, 1), (8 + index) % raw_count, f"-850.{index + 20:02d}")
            for index in range(4)
        )
        rows.append((month_day(5, 2), 0, "825.00"))
        rows.append((month_day(5, 2), 0, "-825.00"))
    elif shape == "salary_rise":
        rows = [(month_day(index, 28), 0, "2350.00") for index in range(8)]
        rows.extend((month_day(8 + index, 28), 0, "2425.00") for index in range(4))
    elif shape == "season_refund_multi":
        rows = [
            (date(2024, 8, 20), 0, "-510.00"),
            (date(2025, 8, 20), 1, "-540.00"),
            (date(2025, 8, 22), 2, "540.00"),
            (date(2025, 8, 23), 2, "-540.00"),
        ]
    elif shape == "standing_price_multi":
        rows = [
            (
                month_day(index, 3),
                index % raw_count,
                f"-125.{index + 10:02d}" if index < 8 else f"-140.{index + 10:02d}",
            )
            for index in range(12)
        ]
        rows.append((month_day(4, 3), 0, "-125.00"))
    elif shape == "grocery_refund_multi":
        rows = [
            (date(2025, 1, 4) + timedelta(days=7 * index), index % raw_count, f"-{41 + index}.{(index * 13) % 100:02d}")
            for index in range(12)
        ]
        rows.append((date(2025, 2, 16), 1, "18.25"))
        rows.append((date(2025, 2, 16), 1, "-18.25"))
    elif shape == "duplicate_refund":
        rows = [
            (date(2025, 4, 9), 0, "-74.00"),
            (date(2025, 4, 9), 0, "-74.00"),
            (date(2025, 4, 11), 0, "74.00"),
        ]
    elif shape == "chargeback":
        rows = [(date(2025, 6, 2), 0, "-63.20"), (date(2025, 6, 9), 0, "63.20")]
    elif shape == "market_refund_multi":
        rows = [
            (date(2025, 2, 6), 0, "-22.40"),
            (date(2025, 5, 18), 1, "-31.10"),
            (date(2025, 5, 20), 2, "31.10"),
        ]
    elif shape == "duplicate":
        rows = [(date(2025, 7, 12), 0, "-96.00"), (date(2025, 7, 12), 0, "-96.00")]
    else:
        raise ValueError(f"unknown transaction shape: {shape}")

    return rows, period


def item_material(
    eval_set: str,
    base: str,
    family: str,
    token: str,
    pattern: dict,
) -> tuple[list[dict], list[dict], list[dict], list[list[str]]]:
    merchant = merchant_name(token, pattern["noun"])
    multi = "multi-descriptor-merchant" in pattern["strata"]
    descriptors = (
        processor_descriptors(base, merchant)
        if multi
        else [("primary", descriptor(base, merchant))]
    )
    rows, period = transaction_shape(pattern["shape"], len(descriptors))
    item_stem = f"{eval_set}-{base}-{family}-{token}-{pattern['id']}"
    strata = [base, family, *pattern["strata"]]

    normalise = [{"raw": raw, "name": merchant} for _processor, raw in descriptors]
    # Classification scores one household decision. The other raw
    # descriptors remain first-class normalise inputs for Stage 5's
    # merge/split clustering metric; counting them here as three
    # independent decisions would exaggerate the evidence.
    classify = [
        {
            "id": item_stem,
            "strata": strata,
            "raw": descriptors[0][1],
            "name": merchant,
            "kind": pattern["kind"],
            "category": pattern["category"],
        }
    ]
    recurring = [{"merchant": merchant, "period": period}] if period else []
    csv_rows = [
        [when.isoformat(), descriptors[raw_index][1], amount]
        for when, raw_index, amount in rows
    ]
    return normalise, classify, recurring, csv_rows


def render_fixture(
    spec: dict,
    eval_set: str,
    base: str,
    family: str,
    token: str,
    patterns: list[dict],
) -> dict[Path, str]:
    fixture_id = f"{eval_set}-generated-{base}-{family}-{token}"
    stem = f"{GENERATED_PREFIX}{eval_set}-{base}-{family}-{token}"
    normalise: list[dict] = []
    classify: list[dict] = []
    recurring: list[dict] = []
    rows: list[list[str]] = []
    for pattern in patterns:
        material = item_material(eval_set, base, family, token, pattern)
        normalise.extend(material[0])
        classify.extend(material[1])
        recurring.extend(material[2])
        rows.extend(material[3])

    assert len({item["id"] for item in classify}) == len(classify)
    assert len(classify) <= 14
    rows.sort(key=lambda row: (row[0], row[1], row[2]))

    csv_output = io.StringIO(newline="")
    writer = csv.writer(csv_output, lineterminator="\n")
    writer.writerow(["Date", "Description", "Amount"])
    writer.writerows(rows)
    expected = {
        "fixture_id": fixture_id,
        "eval_set": eval_set,
        "normalise": normalise,
        "classify": classify,
        "recurring": recurring,
        "tolerances": {
            "normalise": "fuzzy:0.85",
            "classify_kind": "exact",
            "classify_category": "exact",
            "recurring": "exact",
        },
    }
    return {
        HERE / f"{stem}.csv": csv_output.getvalue(),
        HERE / f"{stem}.expected.json": json.dumps(expected, indent=2) + "\n",
    }


def outputs(spec: dict) -> dict[Path, str]:
    subscription_patterns = spec["subscription_patterns"]
    negative_patterns = {pattern["id"]: pattern for pattern in spec["negative_patterns"]}
    rendered: dict[Path, str] = {}

    for eval_set, set_spec in spec["sets"].items():
        for base, cohorts in set_spec["heavy"].items():
            for pair_id, tokens in cohorts.items():
                negatives = [
                    negative_patterns[pattern_id]
                    for pattern_id in spec["negative_pairs"][pair_id]
                ]
                for token in tokens:
                    rendered.update(
                        render_fixture(
                            spec,
                            eval_set,
                            base,
                            "subscription-heavy",
                            token,
                            [*subscription_patterns, *negatives],
                        )
                    )
        for base, tokens in set_spec["no_subscriptions"].items():
            for token in tokens:
                rendered.update(
                    render_fixture(
                        spec,
                        eval_set,
                        base,
                        "no-subscriptions",
                        token,
                        list(negative_patterns.values()),
                    )
                )

    item_ids: set[str] = set()
    for path, content in rendered.items():
        if not path.name.endswith(".expected.json"):
            continue
        for item in json.loads(content)["classify"]:
            assert item["id"] not in item_ids, f"duplicate authored item id: {item['id']}"
            item_ids.add(item["id"])
    assert len(rendered) == 308, f"expected 154 fixture pairs, got {len(rendered) // 2}"
    return rendered


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="refuse generated files that differ instead of rewriting them",
    )
    args = parser.parse_args()
    spec = json.loads(SPEC_PATH.read_text(encoding="utf-8"))
    rendered = outputs(spec)

    if args.check:
        drifted = [
            path.name
            for path, content in rendered.items()
            if not path.exists() or path.read_text(encoding="utf-8") != content
        ]
        stale = [
            path.name
            for path in HERE.glob(f"{GENERATED_PREFIX}*")
            if path not in rendered
        ]
        if drifted or stale:
            names = ", ".join(sorted([*drifted, *stale])[:10])
            raise SystemExit(f"generated eval bed is out of date: {names}")
        return 0

    for path in HERE.glob(f"{GENERATED_PREFIX}*"):
        if path not in rendered:
            path.unlink()
    for path, content in rendered.items():
        path.write_text(content, encoding="utf-8")
    print(f"wrote {len(rendered) // 2} synthetic statement/expectation pairs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
