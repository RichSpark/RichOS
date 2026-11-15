// THE OPENINGS, AS DATA (round 74: richos-hq design/mockups/rounds/round-74). Every opening
// there is, one row each, in the order they are numbered, and the ladders they belong to.
// An opening is a library entry ("base", or its family's) plus, for a milestone or an edition,
// a number and a line. Adding an opening is adding a row.
//
// "earn" is what makes an opening the user's, and it is the one place those numbers live
// (CEO, 2026-11-14: the thresholds as round 74 has them, to be adjusted later):
//   "start"   his from the first start
//   "spend"   dollars of AI power harnessed, all time (the Token spend screen's own figure)
//   "jobs"    jobs completed, all time
//   "hours"   hours reclaimed in one calendar month
//   "months"  calendar months since the first start
//   "starts"  starts of RichOS
//   "update"  the first start after an update
//
// The shell reads this file as well as the page (`richos-core` `openings::parse`), which is why
// everything after the assignment is plain JSON: no comments, no trailing commas, no code.
window.RichOpeningsCatalog = {
  "families": {
    "house": { "title": "The two openings", "base": "round-11/v1", "seconds": null },
    "power": { "title": "AI power harnessed", "caption": "in AI power harnessed", "base": "round-11/v1", "seconds": "5", "fx": "sparks" },
    "jobs": { "title": "Jobs completed", "caption": "jobs completed", "base": "round-11/v2", "seconds": "5", "fx": "lights" },
    "hours": { "title": "Hours reclaimed", "caption": "hours reclaimed for you last month", "base": "round-11/v1", "seconds": "5", "fx": "roll" },
    "edition": { "title": "Rare editions", "base": "round-11/v2", "seconds": "5", "fx": "seal" },
    "material": { "title": "Materials", "seconds": null }
  },
  "openings": [
    { "id": "ruled-standard", "family": "house", "name": "The Ruled Standard", "base": "round-11/v1", "earn": { "start": true } },
    { "id": "midnight-suede", "family": "house", "name": "Midnight Suede", "base": "round-11/v2", "earn": { "start": true } },

    { "id": "power-100", "family": "power", "name": "First Light", "figure": "$100", "rank": 0.0, "earn": { "spend": 100 } },
    { "id": "power-1k", "family": "power", "name": "Kindling", "figure": "$1,000", "rank": 0.14, "earn": { "spend": 1000 } },
    { "id": "power-10k", "family": "power", "name": "Sparks", "figure": "$10,000", "rank": 0.3, "earn": { "spend": 10000 } },
    { "id": "power-50k", "family": "power", "name": "Tally", "figure": "$50,000", "rank": 0.45, "earn": { "spend": 50000 } },
    { "id": "power-100k", "family": "power", "name": "Struck", "figure": "$100,000", "rank": 0.6, "struck": true, "earn": { "spend": 100000 } },
    { "id": "power-250k", "family": "power", "name": "Bullion", "figure": "$250,000", "rank": 0.75, "struck": true, "earn": { "spend": 250000 } },
    { "id": "power-500k", "family": "power", "name": "The Vault", "figure": "$500,000", "rank": 0.88, "struck": true, "earn": { "spend": 500000 } },
    { "id": "power-1m", "family": "power", "name": "Sovereign", "figure": "$1,000,000", "rank": 1.0, "struck": true, "earn": { "spend": 1000000 } },

    { "id": "jobs-1", "family": "jobs", "name": "The First Job", "figure": "1", "caption": "job completed", "rank": 0.0, "earn": { "jobs": 1 } },
    { "id": "jobs-10", "family": "jobs", "name": "Ten", "figure": "10", "rank": 0.2, "earn": { "jobs": 10 } },
    { "id": "jobs-100", "family": "jobs", "name": "A Hundred Lights", "figure": "100", "rank": 0.4, "earn": { "jobs": 100 } },
    { "id": "jobs-500", "family": "jobs", "name": "Every Job, a Light", "figure": "500", "rank": 0.6, "earn": { "jobs": 500 } },
    { "id": "jobs-1000", "family": "jobs", "name": "A Thousand Lights", "figure": "1,000", "rank": 0.8, "earn": { "jobs": 1000 } },
    { "id": "jobs-5000", "family": "jobs", "name": "Constellation", "figure": "5,000", "rank": 1.0, "struck": true, "earn": { "jobs": 5000 } },

    { "id": "hours-12", "family": "hours", "name": "Half a Day", "figure": "12", "rank": 0.0, "earn": { "hours": 12 } },
    { "id": "hours-16", "family": "hours", "name": "Two Workdays", "figure": "16", "rank": 0.33, "earn": { "hours": 16 } },
    { "id": "hours-20", "family": "hours", "name": "Odometer", "figure": "20", "rank": 0.66, "earn": { "hours": 20 } },
    { "id": "hours-40", "family": "hours", "name": "A Full Week", "figure": "40", "rank": 1.0, "struck": true, "earn": { "hours": 40 } },

    { "id": "edition-update", "family": "edition", "name": "Fresh from the Update", "figure": "{version}", "caption": "RichOS, updated and ready", "still": true, "rank": 0.3, "earn": { "update": true } },
    { "id": "edition-year", "family": "edition", "name": "One Year with Rich", "figure": "1 year", "caption": "with Rich, to the day", "still": true, "rank": 0.7, "struck": true, "earn": { "months": 12 } },
    { "id": "edition-1000", "family": "edition", "name": "The Thousandth Start", "figure": "1,000th", "caption": "start of RichOS", "rank": 0.6, "struck": true, "earn": { "starts": 1000 } },

    { "id": "material-leather", "family": "material", "name": "Bridle Leather", "base": "round-8.1/v8", "after": "3 months with Rich", "earn": { "months": 3 } },
    { "id": "material-slate", "family": "material", "name": "Honed Slate", "base": "round-8.1/v11", "after": "6 months with Rich", "earn": { "months": 6 } },
    { "id": "material-steel", "family": "material", "name": "Blued Steel", "base": "round-8.1/v13", "after": "1 year with Rich", "earn": { "months": 12 } },
    { "id": "material-enamel", "family": "material", "name": "Guilloché Enamel", "base": "round-8.1/v16", "after": "2 years with Rich", "earn": { "months": 24 } },
    { "id": "material-leaf", "family": "material", "name": "Gold Leaf", "base": "round-8.1/v17", "after": "3 years with Rich", "earn": { "months": 36 } }
  ]
};
