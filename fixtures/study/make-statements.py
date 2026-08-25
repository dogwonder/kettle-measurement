#!/usr/bin/env python3
"""Generate the ten study statements (#431).

The human study shows each participant ten reports. They have to be ten
*distinguishable* reports: ten seeds spread over one document shows the
same five merchants ten times, and by the fourth report the participant
is studying the harness rather than reading a report.

These are the inputs. The reports themselves come from running the
subscription pack on them for real, because a report Kettle would
produce is not the same artefact as a report Kettle did produce —
decided 25 August 2026: any error the pipeline makes on its own tells
the study more than a corpus with none. Natural errors are recorded in
the corpus audit, never assumed absent, and a report carrying one cannot
serve as a clean control.

Everything is invented: invented amounts, invented dates, invented
sequences, no row traceable to anybody (CLAUDE.md). Real public brands
appear as descriptor text only, per the 30 July 2026 amendment — the
word "Netflix" discloses nothing about anyone — beside a long tail of
invented merchants nobody could recognise, because "I don't know this
one, surface it" is a correct answer the study must let a report give.

Each statement carries, by construction:

- at least four recurring commitments, so there is always something to
  seed;
- at least one price rise no earlier than February, so
  `wrong-rise-month` always has a target with a payment in the month
  before it;
- at least one non-yearly commitment, so `wrong-period` always has one;
- a long tail of one-off spending and at least one income line.

    python3 fixtures/study/make-statements.py
"""

from pathlib import Path

HERE = Path(__file__).parent

# (day of month, descriptor, monthly amount, months, rise)
#   months: which months of 2025 it is paid in
#   rise:   (month, new amount) or None
MONTHLY = list(range(1, 13))
QUARTERLY = [1, 4, 7, 10]
YEARLY = [3]

Statement = dict


def commitment(day, descriptor, amount, months=None, rise=None, variants=None):
    return {
        "day": day,
        "descriptor": descriptor,
        "amount": amount,
        "months": months or MONTHLY,
        "rise": rise,
        "variants": variants,
    }


STATEMENTS: list[Statement] = [
    # 01 — a flat share in a city. Ordinary subscriptions, a quarterly
    # water bill, rent to an invented letting agent.
    {
        "name": "statement-01",
        "note": "flat share: subscriptions, quarterly water, rent",
        "commitments": [
            commitment(2, "CASTLE HILL LETTINGS", 780.00),
            commitment(5, "NETFLIX.COM", 10.99, rise=(6, 12.99)),
            commitment(12, "SPOTIFY LTD", 11.99),
            commitment(3, "PUREGYM LTD", 24.99),
            commitment(18, "THAMES WATER", 96.40, months=QUARTERLY),
        ],
        "income": (28, "MERIDIAN LOGISTICS PAYROLL", 2450.00),
        "one_offs": [
            (1, 20, "TESCO STORES 3412", 42.17),
            (2, 27, "TESCO STORES 3412", 38.05),
            (3, 9, "J HENDERSON WINDOWS", 185.00),
            (5, 14, "TESCO STORES 3412", 55.60),
            (7, 2, "COASTLINE TRAVEL", 320.00),
            (9, 19, "TESCO STORES 3412", 47.88),
            (11, 6, "BRAMBLE & CO HARDWARE", 63.40),
        ],
    },
    # 02 — a family. A yearly car insurance, monthly energy, a nursery.
    {
        "name": "statement-02",
        "note": "family: yearly insurance, monthly energy, nursery",
        "commitments": [
            commitment(6, "ORCHARD LANE NURSERY", 642.00, rise=(9, 671.00)),
            commitment(4, "DISNEY PLUS", 7.99),
            commitment(11, "SKY BROADBAND", 38.00),
            commitment(15, "BRITISH GAS", 121.50),
            commitment(21, "AVIVA MOTOR", 486.00, months=YEARLY),
        ],
        "income": (25, "PENWORTHY TRUST PAYROLL", 3120.00),
        "one_offs": [
            (1, 13, "SAINSBURYS 2287", 88.32),
            (2, 8, "HALFORDS 0912", 42.99),
            (4, 22, "SAINSBURYS 2287", 91.05),
            (6, 30, "KESTREL SOFT PLAY", 24.00),
            (8, 17, "SAINSBURYS 2287", 102.44),
            (10, 3, "WYNDHAM DENTAL PRACTICE", 78.00),
            (12, 12, "SAINSBURYS 2287", 134.60),
        ],
    },
    # 03 — a freelancer. Software subscriptions with descriptor noise,
    # invoices in rather than a salary.
    {
        "name": "statement-03",
        "note": "freelancer: software subscriptions, descriptor noise, invoices in",
        "commitments": [
            commitment(
                7,
                "ADOBE CREATIVE CLOUD",
                61.30,
                rise=(8, 65.98),
                variants={1: "STRIPE* ADOBE", 5: "SQ *ADOBE CC"},
            ),
            commitment(9, "DROPBOX", 9.99),
            commitment(14, "XERO ACCOUNTING", 33.00),
            commitment(2, "KINGSGATE WORKSPACE", 210.00),
            commitment(20, "HISCOX PROFESSIONAL", 39.60, months=QUARTERLY),
        ],
        "income": None,
        "one_offs": [
            (1, 28, "FAIRWEATHER STUDIO INVOICE", -1840.00),
            (3, 26, "FAIRWEATHER STUDIO INVOICE", -2210.00),
            (4, 11, "HMRC PAYMENT ON ACCOUNT", 1450.00),
            (5, 29, "LARKSPUR MEDIA INVOICE", -1975.00),
            (7, 30, "FAIRWEATHER STUDIO INVOICE", -2040.00),
            (9, 24, "LARKSPUR MEDIA INVOICE", -1620.00),
            (10, 11, "HMRC PAYMENT ON ACCOUNT", 1450.00),
            (11, 27, "FAIRWEATHER STUDIO INVOICE", -2380.00),
        ],
    },
    # 04 — a student. Small amounts, a yearly membership, halls.
    {
        "name": "statement-04",
        "note": "student: small amounts, yearly membership, halls",
        "commitments": [
            commitment(1, "BRIARWOOD HALLS", 498.00),
            commitment(8, "AMAZON PRIME", 8.99, rise=(4, 9.99)),
            commitment(16, "SPOTIFY LTD", 5.99),
            commitment(23, "DUOLINGO PLUS", 59.99, months=YEARLY),
            commitment(5, "CAMPUS SPORT MEMBERSHIP", 19.50),
        ],
        "income": (30, "STUDENT FINANCE ENGLAND", 1420.00),
        "one_offs": [
            (2, 4, "ALDI 1183", 27.40),
            (2, 19, "THE PAPER MOON BOOKSHOP", 18.99),
            (4, 7, "ALDI 1183", 31.05),
            (6, 15, "NORTHGATE LAUNDRY", 12.00),
            (8, 21, "ALDI 1183", 24.70),
            (10, 9, "TIDEWAY COACH TRAVEL", 46.50),
            (11, 30, "ALDI 1183", 35.15),
        ],
    },
    # 05 — a retiree. A yearly insurance, a landline, small memberships,
    # pension in.
    {
        "name": "statement-05",
        "note": "retiree: yearly insurance, landline, memberships, pension in",
        "commitments": [
            commitment(3, "BT LANDLINE", 32.40, rise=(5, 35.90)),
            commitment(10, "SAGA HOME INSURANCE", 318.00, months=YEARLY),
            commitment(17, "NATIONAL TRUST", 84.00, months=YEARLY),
            commitment(6, "ST CUTHBERTS PARISH MAGAZINE", 4.50),
            commitment(24, "SEVERN TRENT WATER", 61.20, months=QUARTERLY),
        ],
        "income": (26, "HAWKRIDGE PENSION SCHEME", 1680.00),
        "one_offs": [
            (1, 9, "MORRISONS 0447", 52.18),
            (3, 14, "PENNYFIELD GARDEN CENTRE", 38.75),
            (5, 2, "MORRISONS 0447", 44.90),
            (6, 27, "ELMTREE OPTICIANS", 145.00),
            (8, 8, "MORRISONS 0447", 58.32),
            (9, 20, "HOLLOWAY COACH TOURS", 265.00),
            (12, 5, "MORRISONS 0447", 71.44),
        ],
    },
    # 06 — a small trader. Card fees, wholesale, quarterly liability
    # cover, takings in.
    {
        "name": "statement-06",
        "note": "small trader: card fees, wholesale, quarterly cover, takings in",
        "commitments": [
            commitment(4, "SQ *CARD FEES", 47.80),
            commitment(12, "EE MOBILE", 28.00, rise=(7, 31.50)),
            commitment(19, "MERCHANT LIABILITY COVER", 138.00, months=QUARTERLY),
            commitment(2, "WHITTAKER YARD RENT", 425.00),
            commitment(27, "BOOKER WHOLESALE ACCOUNT", 612.40),
        ],
        "income": None,
        "one_offs": [
            (1, 31, "CARD TAKINGS SETTLEMENT", -2870.00),
            (2, 28, "CARD TAKINGS SETTLEMENT", -2410.00),
            (3, 31, "CARD TAKINGS SETTLEMENT", -3105.00),
            (4, 30, "CARD TAKINGS SETTLEMENT", -2760.00),
            (6, 30, "CARD TAKINGS SETTLEMENT", -3320.00),
            (8, 29, "CARD TAKINGS SETTLEMENT", -2985.00),
            (10, 31, "CARD TAKINGS SETTLEMENT", -3440.00),
            (5, 16, "REDGRAVE SIGNWRITING", 340.00),
            (11, 14, "COLDSPRING REFRIGERATION", 780.00),
        ],
    },
    # 07 — a new parent. A nappy subscription, classes, statutory pay.
    {
        "name": "statement-07",
        "note": "new parent: subscription boxes, classes, statutory pay in",
        "commitments": [
            commitment(5, "BUBBLE NAPPY SUBSCRIPTION", 34.99, rise=(10, 38.99)),
            commitment(13, "LITTLE ACORNS SENSORY", 48.00),
            commitment(8, "VODAFONE", 22.00),
            commitment(1, "MEADOWBANK LETTINGS", 895.00),
            commitment(22, "WATERSIDE PHYSIOTHERAPY", 55.00, months=[2, 3, 4, 5]),
        ],
        "income": (27, "STATUTORY MATERNITY PAY", 1210.00),
        "one_offs": [
            (1, 17, "BOOTS 2231", 34.60),
            (3, 6, "MOTHERCARE ONLINE", 128.00),
            (4, 25, "BOOTS 2231", 41.20),
            (6, 11, "KESTREL PHOTOGRAPHY", 95.00),
            (8, 3, "BOOTS 2231", 28.75),
            (9, 28, "HARLOW & SONS NURSERY FURNITURE", 310.00),
            (12, 2, "BOOTS 2231", 52.40),
        ],
    },
    # 08 — a commuter and cyclist. A season ticket, a yearly membership,
    # a servicing plan.
    {
        "name": "statement-08",
        "note": "commuter: season ticket, yearly membership, servicing plan",
        "commitments": [
            commitment(1, "TRAINLINE SEASON", 312.00, rise=(3, 331.00)),
            commitment(9, "BRITISH CYCLING", 78.00, months=YEARLY),
            commitment(15, "ZWIFT", 12.99),
            commitment(6, "HALFORDS SERVICE PLAN", 14.50),
            commitment(20, "OCTOPUS ENERGY", 94.30),
        ],
        "income": (28, "GRANTHAM & VALE PAYROLL", 2890.00),
        "one_offs": [
            (2, 12, "LIDL 0871", 39.90),
            (3, 22, "THE COG & SPOKE WORKSHOP", 118.00),
            (5, 5, "LIDL 0871", 44.15),
            (7, 19, "SUMMIT OUTDOOR SUPPLIES", 205.00),
            (9, 8, "LIDL 0871", 36.70),
            (10, 26, "THE COG & SPOKE WORKSHOP", 92.50),
            (12, 15, "LIDL 0871", 61.30),
        ],
    },
    # 09 — a musician. Rehearsal room, a union, gig income, and two
    # streaming services with descriptor noise.
    {
        "name": "statement-09",
        "note": "musician: rehearsal room, quarterly union, gig income, noisy descriptors",
        "commitments": [
            commitment(
                2,
                "SPOTIFY LTD",
                16.99,
                variants={4: "PAYPAL *SPOTIFY", 8: "SPOTIFY P16.99"},
            ),
            commitment(11, "SPLICE SOUNDS", 9.99, rise=(6, 11.99)),
            commitment(7, "TANNERY REHEARSAL ROOMS", 165.00),
            commitment(18, "MUSICIANS UNION", 51.00, months=QUARTERLY),
            commitment(25, "FERNBANK STORAGE", 44.00),
        ],
        "income": None,
        "one_offs": [
            (1, 24, "THE LAMPLIGHTER GIG FEE", -280.00),
            (2, 21, "WESTGATE ARTS GIG FEE", -450.00),
            (4, 18, "THE LAMPLIGHTER GIG FEE", -280.00),
            (5, 30, "HARBOUR FESTIVAL FEE", -1100.00),
            (7, 12, "WESTGATE ARTS GIG FEE", -450.00),
            (9, 27, "THE LAMPLIGHTER GIG FEE", -320.00),
            (11, 8, "CATHEDRAL QUARTER SESSIONS", -675.00),
            (3, 15, "STRINGS & THINGS REPAIR", 88.00),
            (10, 4, "MERIDIAN AMP SERVICING", 145.00),
        ],
    },
    # 10 — a carer. Council tax over ten months, a small membership, a
    # taxi account, an allowance in.
    {
        "name": "statement-10",
        "note": "carer: council tax over ten months, taxi account, allowance in",
        "commitments": [
            commitment(1, "BOROUGH COUNCIL TAX", 168.00, months=list(range(1, 11))),
            commitment(14, "CARERS UK MEMBERSHIP", 36.00, months=YEARLY),
            commitment(9, "ASHDOWN TAXI ACCOUNT", 96.50, rise=(8, 104.00)),
            commitment(4, "PHARMACY DELIVERY SERVICE", 12.00),
            commitment(19, "SOUTHERN WATER", 58.90, months=QUARTERLY),
        ],
        "income": (23, "CARERS ALLOWANCE", 332.00),
        "one_offs": [
            (1, 11, "ICELAND 0338", 46.20),
            (2, 26, "MOBILITY DIRECT", 189.00),
            (4, 14, "ICELAND 0338", 38.95),
            (6, 6, "BRIARFIELD DAY CENTRE", 60.00),
            (8, 23, "ICELAND 0338", 51.10),
            (10, 17, "HAZELMERE CHIROPODY", 42.00),
            (12, 9, "ICELAND 0338", 44.85),
        ],
    },
]


def rows_for(statement: Statement) -> list[tuple[str, str, str]]:
    rows: list[tuple[str, str, str]] = []

    for item in statement["commitments"]:
        for month in item["months"]:
            amount = item["amount"]
            if item["rise"] is not None:
                rise_month, risen = item["rise"]
                if month >= rise_month:
                    amount = risen
            descriptor = item["descriptor"]
            if item["variants"] is not None:
                descriptor = item["variants"].get(month, descriptor)
            rows.append(
                (f"2025-{month:02d}-{item['day']:02d}", descriptor, f"-{amount:.2f}")
            )

    if statement["income"] is not None:
        day, descriptor, amount = statement["income"]
        for month in MONTHLY:
            rows.append((f"2025-{month:02d}-{day:02d}", descriptor, f"{amount:.2f}"))

    for month, day, descriptor, amount in statement["one_offs"]:
        # A negative amount in the spec means money in — an invoice paid,
        # a gig fee, a card settlement — and keeps the sign convention
        # of the file, which is "minus is money out".
        sign = "" if amount < 0 else "-"
        rows.append((f"2025-{month:02d}-{day:02d}", descriptor, f"{sign}{abs(amount):.2f}"))

    rows.sort(key=lambda row: (row[0], row[1]))
    return rows


def main() -> None:
    for index, statement in enumerate(STATEMENTS, start=1):
        rows = rows_for(statement)
        path = HERE / f"{statement['name']}.csv"
        lines = ["Date,Description,Amount"]
        lines += [f"{date},{descriptor},{amount}" for date, descriptor, amount in rows]
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"{path.name}: {len(rows)} rows — {statement['note']}")


if __name__ == "__main__":
    main()
