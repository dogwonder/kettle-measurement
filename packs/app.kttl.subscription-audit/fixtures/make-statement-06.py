#!/usr/bin/env python3
"""Generate the broad eval fixture added for #234.

Eighty distinct synthetic merchants make one completely wrong merchant
worth 1.25 percentage points, rather than the 10 points it is worth in
each original five-merchant fixture. There is one transaction per
merchant: the fixture measures naming and sorting breadth without
creating recurrence expectations for invented payment histories.

Everything is invented. Never a real export (CLAUDE.md).

    python3 packs/app.kttl.subscription-audit/fixtures/make-statement-06.py
"""

import csv
import json
from datetime import date, timedelta
from pathlib import Path

HERE = Path(__file__).parent

# raw statement spelling, expected display name, kind, category
MERCHANTS = [
    # Automatically billed services.
    (
        "NORTHSTAR STREAMING SUBSCRIPTION",
        "Northstar Streaming Subscription",
        "subscription",
        "streaming",
    ),
    (
        "FIREFLY FILMS SUBSCRIPTION",
        "Firefly Films Subscription",
        "subscription",
        "streaming",
    ),
    (
        "HARBOUR TV SUBSCRIPTION",
        "Harbour TV Subscription",
        "subscription",
        "streaming",
    ),
    (
        "ORCHARD MUSIC SUBSCRIPTION",
        "Orchard Music Subscription",
        "subscription",
        "streaming",
    ),
    (
        "PAPERPLANE CLOUD SUBSCRIPTION",
        "Paperplane Cloud Subscription",
        "subscription",
        "software",
    ),
    (
        "LANTERN NOTES SUBSCRIPTION",
        "Lantern Notes Subscription",
        "subscription",
        "software",
    ),
    (
        "COPPER CRM SUBSCRIPTION",
        "Copper CRM Subscription",
        "subscription",
        "software",
    ),
    (
        "WILLOW PASSWORDS SUBSCRIPTION",
        "Willow Passwords Subscription",
        "subscription",
        "software",
    ),
    (
        "MORNING LEDGER SUBSCRIPTION",
        "Morning Ledger Subscription",
        "subscription",
        "news_media",
    ),
    (
        "CITY JOURNAL SUBSCRIPTION",
        "City Journal Subscription",
        "subscription",
        "news_media",
    ),
    (
        "WEEKLY REVIEW SUBSCRIPTION",
        "Weekly Review Subscription",
        "subscription",
        "news_media",
    ),
    (
        "SCIENCE MONTHLY SUBSCRIPTION",
        "Science Monthly Subscription",
        "subscription",
        "news_media",
    ),
    (
        "RIVER YOGA SUBSCRIPTION",
        "River Yoga Subscription",
        "subscription",
        "fitness",
    ),
    (
        "SUMMIT FITNESS SUBSCRIPTION",
        "Summit Fitness Subscription",
        "subscription",
        "fitness",
    ),
    (
        "CYCLE COACH SUBSCRIPTION",
        "Cycle Coach Subscription",
        "subscription",
        "fitness",
    ),
    (
        "HOME PILATES SUBSCRIPTION",
        "Home Pilates Subscription",
        "subscription",
        "fitness",
    ),
    (
        "SEASIDE AUDIOBOOK SUBSCRIPTION",
        "Seaside Audiobook Subscription",
        "subscription",
        "news_media",
    ),
    (
        "MAPLE LANGUAGE APP SUBSCRIPTION",
        "Maple Language App Subscription",
        "subscription",
        "software",
    ),
    (
        "NIGHT OWL SLEEP APP SUBSCRIPTION",
        "Night Owl Sleep App Subscription",
        "subscription",
        "fitness",
    ),
    (
        "ARCADE GAME PASS SUBSCRIPTION",
        "Arcade Game Pass Subscription",
        "subscription",
        "software",
    ),
    # Essential recurring bills.
    ("NORTH COUNTY ENERGY", "North County Energy", "utility", "energy"),
    ("SUNWARD ELECTRIC", "Sunward Electric", "utility", "energy"),
    ("HEARTH GAS", "Hearth Gas", "utility", "energy"),
    ("GREEN GRID POWER", "Green Grid Power", "utility", "energy"),
    ("CITY WATER BILL", "City Water Bill", "utility", "other"),
    ("RIVER WATER SERVICES", "River Water Services", "utility", "other"),
    (
        "HOME INSURANCE MONTHLY PREMIUM",
        "Home Insurance Monthly Premium",
        "utility",
        "insurance",
    ),
    (
        "MOTOR COVER MONTHLY PREMIUM",
        "Motor Cover Monthly Premium",
        "utility",
        "insurance",
    ),
    (
        "PET COVER MONTHLY PREMIUM",
        "Pet Cover Monthly Premium",
        "utility",
        "insurance",
    ),
    (
        "TRAVEL COVER ANNUAL PREMIUM",
        "Travel Cover Annual Premium",
        "utility",
        "insurance",
    ),
    ("OAK STREET MONTHLY RENT", "Oak Street Monthly Rent", "utility", "housing"),
    (
        "HARBOUR MORTGAGE REPAYMENT",
        "Harbour Mortgage Repayment",
        "utility",
        "housing",
    ),
    ("CITY COUNCIL TAX BILL", "City Council Tax Bill", "utility", "housing"),
    (
        "BUILDING MONTHLY SERVICE CHARGE",
        "Building Monthly Service Charge",
        "utility",
        "housing",
    ),
    (
        "COMMUNITY CREDIT LOAN REPAYMENT",
        "Community Credit Loan Repayment",
        "utility",
        "finance",
    ),
    (
        "STUDENT LOAN REPAYMENT",
        "Student Loan Repayment",
        "utility",
        "finance",
    ),
    (
        "FAMILY LIFE MONTHLY PREMIUM",
        "Family Life Monthly Premium",
        "utility",
        "insurance",
    ),
    ("BOILER CARE MONTHLY PLAN", "Boiler Care Monthly Plan", "utility", "housing"),
    ("LANDLINE MONTHLY BILL", "Landline Monthly Bill", "utility", "telecoms"),
    ("MUNICIPAL HEATING", "Municipal Heating", "utility", "energy"),
    # Habitual, manually initiated spending.
    (
        "CORNER CUP WEEKDAY COFFEE",
        "Corner Cup Weekday Coffee",
        "regular_spend",
        "food_drink",
    ),
    ("MARKET WEEKLY BAKERY", "Market Weekly Bakery", "regular_spend", "food_drink"),
    ("LUNCHBOX WORKDAY CAFE", "Lunchbox Workday Cafe", "regular_spend", "food_drink"),
    ("GREEN WEEKLY GROCER", "Green Weekly Grocer", "regular_spend", "food_drink"),
    ("RIVERSIDE WEEKDAY DELI", "Riverside Weekday Deli", "regular_spend", "food_drink"),
    (
        "STATION WORKDAY CANTEEN",
        "Station Workday Canteen",
        "regular_spend",
        "food_drink",
    ),
    (
        "WEEKLY SUPERMARKET SHOP",
        "Weekly Supermarket Shop",
        "regular_spend",
        "food_drink",
    ),
    (
        "FRIDAY TAKEAWAY",
        "Friday Takeaway",
        "regular_spend",
        "food_drink",
    ),
    ("DAILY CITY BUS", "Daily City Bus", "regular_spend", "transport"),
    ("WEEKLY METRO TOPUP", "Weekly Metro Topup", "regular_spend", "transport"),
    ("WORKDAY PARKING APP", "Workday Parking App", "regular_spend", "transport"),
    ("COMMUTER FUEL", "Commuter Fuel", "regular_spend", "transport"),
    ("WEEKLY BIKE HIRE", "Weekly Bike Hire", "regular_spend", "transport"),
    (
        "COMMUTER FERRY TICKETS",
        "Commuter Ferry Tickets",
        "regular_spend",
        "transport",
    ),
    ("LOCAL PHARMACY", "Local Pharmacy", "regular_spend", "retail"),
    ("HOMEWARE SHOP", "Homeware Shop", "regular_spend", "retail"),
    ("STATIONERY STORE", "Stationery Store", "regular_spend", "retail"),
    ("HIGH STREET BOOKS", "High Street Books", "regular_spend", "retail"),
    ("PET SUPPLIES SHOP", "Pet Supplies Shop", "regular_spend", "retail"),
    ("LOCAL HARDWARE", "Local Hardware", "regular_spend", "retail"),
    # Isolated purchases and payments.
    (
        "FURNITURE WAREHOUSE PURCHASE",
        "Furniture Warehouse Purchase",
        "one_off",
        "retail",
    ),
    (
        "ELECTRONICS OUTLET PURCHASE",
        "Electronics Outlet Purchase",
        "one_off",
        "retail",
    ),
    (
        "GARDEN SHED PURCHASE",
        "Garden Shed Purchase",
        "one_off",
        "retail",
    ),
    ("FORMALWEAR PURCHASE", "Formalwear Purchase", "one_off", "retail"),
    ("KITCHEN APPLIANCE SALE", "Kitchen Appliance Sale", "one_off", "retail"),
    ("AIRLINE TICKET", "Airline Ticket", "one_off", "transport"),
    ("HOLIDAY RAIL BOOKING", "Holiday Rail Booking", "one_off", "transport"),
    ("CAR HIRE WEEKEND", "Car Hire Weekend", "one_off", "transport"),
    ("AIRPORT TRANSFER", "Airport Transfer", "one_off", "transport"),
    ("COMMUNITY FUND DONATION", "Community Fund Donation", "one_off", "charity"),
    ("WILDLIFE APPEAL", "Wildlife Appeal", "one_off", "charity"),
    ("LOCAL FOODBANK GIFT", "Local Foodbank Gift", "one_off", "charity"),
    ("MARATHON SPONSORSHIP", "Marathon Sponsorship", "one_off", "charity"),
    ("ROOF REPAIR", "Roof Repair", "one_off", "housing"),
    ("WINDOW INSTALLATION", "Window Installation", "one_off", "housing"),
    ("BANK TRANSFER FEE", "Bank Transfer Fee", "one_off", "finance"),
    ("PASSPORT OFFICE", "Passport Office", "one_off", "other"),
    ("LOCAL SOLICITOR", "Local Solicitor", "one_off", "other"),
    ("WEDDING PHOTOGRAPHER", "Wedding Photographer", "one_off", "other"),
    ("DENTAL CLINIC", "Dental Clinic", "one_off", "other"),
]

# Authored, immutable scored-item identity. Keying this by the raw
# merchant keeps it stable if MERCHANTS is reordered; deriving ids while
# emitting would only hide ordinal identity inside the generator (#237).
ITEM_METADATA = {
    "NORTHSTAR STREAMING SUBSCRIPTION": (
        "broad-northstar-streaming-01",
        ["broad", "automatic-billing"],
    ),
    "FIREFLY FILMS SUBSCRIPTION": (
        "broad-firefly-films-01",
        ["broad", "automatic-billing"],
    ),
    "HARBOUR TV SUBSCRIPTION": ("broad-harbour-tv-01", ["broad", "automatic-billing"]),
    "ORCHARD MUSIC SUBSCRIPTION": (
        "broad-orchard-music-01",
        ["broad", "automatic-billing"],
    ),
    "PAPERPLANE CLOUD SUBSCRIPTION": (
        "broad-paperplane-cloud-01",
        ["broad", "automatic-billing"],
    ),
    "LANTERN NOTES SUBSCRIPTION": (
        "broad-lantern-notes-01",
        ["broad", "automatic-billing"],
    ),
    "COPPER CRM SUBSCRIPTION": ("broad-copper-crm-01", ["broad", "automatic-billing"]),
    "WILLOW PASSWORDS SUBSCRIPTION": (
        "broad-willow-passwords-01",
        ["broad", "automatic-billing"],
    ),
    "MORNING LEDGER SUBSCRIPTION": (
        "broad-morning-ledger-01",
        ["broad", "automatic-billing"],
    ),
    "CITY JOURNAL SUBSCRIPTION": (
        "broad-city-journal-01",
        ["broad", "automatic-billing"],
    ),
    "WEEKLY REVIEW SUBSCRIPTION": (
        "broad-weekly-review-01",
        ["broad", "automatic-billing"],
    ),
    "SCIENCE MONTHLY SUBSCRIPTION": (
        "broad-science-monthly-01",
        ["broad", "automatic-billing"],
    ),
    "RIVER YOGA SUBSCRIPTION": ("broad-river-yoga-01", ["broad", "automatic-billing"]),
    "SUMMIT FITNESS SUBSCRIPTION": (
        "broad-summit-fitness-01",
        ["broad", "automatic-billing"],
    ),
    "CYCLE COACH SUBSCRIPTION": (
        "broad-cycle-coach-01",
        ["broad", "automatic-billing"],
    ),
    "HOME PILATES SUBSCRIPTION": (
        "broad-home-pilates-01",
        ["broad", "automatic-billing"],
    ),
    "SEASIDE AUDIOBOOK SUBSCRIPTION": (
        "broad-seaside-audiobook-01",
        ["broad", "automatic-billing"],
    ),
    "MAPLE LANGUAGE APP SUBSCRIPTION": (
        "broad-maple-language-app-01",
        ["broad", "automatic-billing"],
    ),
    "NIGHT OWL SLEEP APP SUBSCRIPTION": (
        "broad-night-owl-sleep-01",
        ["broad", "automatic-billing"],
    ),
    "ARCADE GAME PASS SUBSCRIPTION": (
        "broad-arcade-game-pass-01",
        ["broad", "automatic-billing"],
    ),
    "NORTH COUNTY ENERGY": (
        "broad-north-county-energy-01",
        ["broad", "essential-bill"],
    ),
    "SUNWARD ELECTRIC": ("broad-sunward-electric-01", ["broad", "essential-bill"]),
    "HEARTH GAS": ("broad-hearth-gas-01", ["broad", "essential-bill"]),
    "GREEN GRID POWER": ("broad-green-grid-power-01", ["broad", "essential-bill"]),
    "CITY WATER BILL": ("broad-city-water-bill-01", ["broad", "essential-bill"]),
    "RIVER WATER SERVICES": (
        "broad-river-water-services-01",
        ["broad", "essential-bill"],
    ),
    "HOME INSURANCE MONTHLY PREMIUM": (
        "broad-home-insurance-premium-01",
        ["broad", "essential-bill"],
    ),
    "MOTOR COVER MONTHLY PREMIUM": (
        "broad-motor-cover-premium-01",
        ["broad", "essential-bill"],
    ),
    "PET COVER MONTHLY PREMIUM": (
        "broad-pet-cover-premium-01",
        ["broad", "essential-bill"],
    ),
    "TRAVEL COVER ANNUAL PREMIUM": (
        "broad-travel-cover-premium-01",
        ["broad", "essential-bill"],
    ),
    "OAK STREET MONTHLY RENT": (
        "broad-oak-street-rent-01",
        ["broad", "essential-bill"],
    ),
    "HARBOUR MORTGAGE REPAYMENT": (
        "broad-harbour-mortgage-01",
        ["broad", "essential-bill"],
    ),
    "CITY COUNCIL TAX BILL": ("broad-city-council-tax-01", ["broad", "essential-bill"]),
    "BUILDING MONTHLY SERVICE CHARGE": (
        "broad-building-service-charge-01",
        ["broad", "essential-bill"],
    ),
    "COMMUNITY CREDIT LOAN REPAYMENT": (
        "broad-community-credit-loan-01",
        ["broad", "essential-bill"],
    ),
    "STUDENT LOAN REPAYMENT": ("broad-student-loan-01", ["broad", "essential-bill"]),
    "FAMILY LIFE MONTHLY PREMIUM": (
        "broad-family-life-premium-01",
        ["broad", "essential-bill"],
    ),
    "BOILER CARE MONTHLY PLAN": (
        "broad-boiler-care-plan-01",
        ["broad", "essential-bill"],
    ),
    "LANDLINE MONTHLY BILL": ("broad-landline-bill-01", ["broad", "essential-bill"]),
    "MUNICIPAL HEATING": ("broad-municipal-heating-01", ["broad", "essential-bill"]),
    "CORNER CUP WEEKDAY COFFEE": (
        "broad-corner-cup-coffee-01",
        ["broad", "habitual-spend"],
    ),
    "MARKET WEEKLY BAKERY": ("broad-market-bakery-01", ["broad", "habitual-spend"]),
    "LUNCHBOX WORKDAY CAFE": ("broad-lunchbox-cafe-01", ["broad", "habitual-spend"]),
    "GREEN WEEKLY GROCER": ("broad-green-grocer-01", ["broad", "habitual-spend"]),
    "RIVERSIDE WEEKDAY DELI": ("broad-riverside-deli-01", ["broad", "habitual-spend"]),
    "STATION WORKDAY CANTEEN": (
        "broad-station-canteen-01",
        ["broad", "habitual-spend"],
    ),
    "WEEKLY SUPERMARKET SHOP": (
        "broad-supermarket-shop-01",
        ["broad", "habitual-spend"],
    ),
    "FRIDAY TAKEAWAY": ("broad-friday-takeaway-01", ["broad", "habitual-spend"]),
    "DAILY CITY BUS": ("broad-city-bus-01", ["broad", "habitual-spend"]),
    "WEEKLY METRO TOPUP": ("broad-metro-topup-01", ["broad", "habitual-spend"]),
    "WORKDAY PARKING APP": ("broad-parking-app-01", ["broad", "habitual-spend"]),
    "COMMUTER FUEL": ("broad-commuter-fuel-01", ["broad", "habitual-spend"]),
    "WEEKLY BIKE HIRE": ("broad-bike-hire-01", ["broad", "habitual-spend"]),
    "COMMUTER FERRY TICKETS": ("broad-ferry-tickets-01", ["broad", "habitual-spend"]),
    "LOCAL PHARMACY": ("broad-local-pharmacy-01", ["broad", "habitual-spend"]),
    "HOMEWARE SHOP": ("broad-homeware-shop-01", ["broad", "habitual-spend"]),
    "STATIONERY STORE": ("broad-stationery-store-01", ["broad", "habitual-spend"]),
    "HIGH STREET BOOKS": ("broad-high-street-books-01", ["broad", "habitual-spend"]),
    "PET SUPPLIES SHOP": ("broad-pet-supplies-01", ["broad", "habitual-spend"]),
    "LOCAL HARDWARE": ("broad-local-hardware-01", ["broad", "habitual-spend"]),
    "FURNITURE WAREHOUSE PURCHASE": (
        "broad-furniture-warehouse-01",
        ["broad", "isolated-purchase"],
    ),
    "ELECTRONICS OUTLET PURCHASE": (
        "broad-electronics-outlet-01",
        ["broad", "isolated-purchase"],
    ),
    "GARDEN SHED PURCHASE": ("broad-garden-shed-01", ["broad", "isolated-purchase"]),
    "FORMALWEAR PURCHASE": (
        "broad-formalwear-purchase-01",
        ["broad", "isolated-purchase"],
    ),
    "KITCHEN APPLIANCE SALE": (
        "broad-kitchen-appliance-01",
        ["broad", "isolated-purchase"],
    ),
    "AIRLINE TICKET": ("broad-airline-ticket-01", ["broad", "isolated-purchase"]),
    "HOLIDAY RAIL BOOKING": ("broad-holiday-rail-01", ["broad", "isolated-purchase"]),
    "CAR HIRE WEEKEND": ("broad-car-hire-01", ["broad", "isolated-purchase"]),
    "AIRPORT TRANSFER": ("broad-airport-transfer-01", ["broad", "isolated-purchase"]),
    "COMMUNITY FUND DONATION": (
        "broad-community-donation-01",
        ["broad", "isolated-purchase"],
    ),
    "WILDLIFE APPEAL": ("broad-wildlife-appeal-01", ["broad", "isolated-purchase"]),
    "LOCAL FOODBANK GIFT": ("broad-foodbank-gift-01", ["broad", "isolated-purchase"]),
    "MARATHON SPONSORSHIP": (
        "broad-marathon-sponsorship-01",
        ["broad", "isolated-purchase"],
    ),
    "ROOF REPAIR": ("broad-roof-repair-01", ["broad", "isolated-purchase"]),
    "WINDOW INSTALLATION": (
        "broad-window-installation-01",
        ["broad", "isolated-purchase"],
    ),
    "BANK TRANSFER FEE": ("broad-bank-transfer-fee-01", ["broad", "isolated-purchase"]),
    "PASSPORT OFFICE": ("broad-passport-office-01", ["broad", "isolated-purchase"]),
    "LOCAL SOLICITOR": ("broad-local-solicitor-01", ["broad", "isolated-purchase"]),
    "WEDDING PHOTOGRAPHER": (
        "broad-wedding-photographer-01",
        ["broad", "isolated-purchase"],
    ),
    "DENTAL CLINIC": ("broad-dental-clinic-01", ["broad", "isolated-purchase"]),
}


def main() -> None:
    assert len(MERCHANTS) == 80
    assert set(ITEM_METADATA) == {raw for raw, _name, _kind, _category in MERCHANTS}
    assert len({item_id for item_id, _strata in ITEM_METADATA.values()}) == 80
    statement = HERE / "statement-06-broad.csv"
    expected = HERE / "statement-06-broad.expected.json"
    first_day = date(2025, 1, 2)

    with statement.open("w", newline="", encoding="utf-8") as output:
        writer = csv.writer(output, lineterminator="\n")
        writer.writerow(["Date", "Description", "Amount"])
        for index, (raw, _name, _kind, _category) in enumerate(MERCHANTS):
            writer.writerow(
                [
                    (first_day + timedelta(days=index)).isoformat(),
                    raw,
                    f"-{8 + index}.{(index * 17) % 100:02d}",
                ]
            )

    answers = {
        "fixture_id": "broad-diverse-merchants-01",
        "normalise": [
            {"raw": raw, "name": name} for raw, name, _kind, _category in MERCHANTS
        ],
        "classify": [
            {
                "id": ITEM_METADATA[raw][0],
                "strata": ITEM_METADATA[raw][1],
                "name": name,
                "kind": kind,
                "category": category,
            }
            for raw, name, kind, category in MERCHANTS
        ],
        "recurring": [],
        "tolerances": {
            "normalise": "fuzzy:0.85",
            "classify_kind": "exact",
            "classify_category": "exact",
            "recurring": "exact",
        },
    }
    expected.write_text(f"{json.dumps(answers, indent=2)}\n", encoding="utf-8")
    print(f"wrote {statement.name} and {expected.name} ({len(MERCHANTS)} merchants)")


if __name__ == "__main__":
    main()
