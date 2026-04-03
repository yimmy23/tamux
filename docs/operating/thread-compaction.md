# 🤖 Agent Context: State Checkpoint [Turn/Timestamp]

## 🎯 Primary Objective
> [1-2 sentences strictly defining the user's end goal. Do not alter this during the run unless explicitly instructed by the user.]

## 🗺️ Execution Map
* **✅ Completed Phase:** [Brief summary of the macro-step just finished]
* **⏳ Current Phase:** [What the agent is currently trying to achieve]
* **⏭️ Pending Phases:** [List of major steps remaining]

## 📁 Working Environment State
* **Active Directory:** `/path/to/current/workdir`
* **Files Modified (Uncommitted/Pending):**
    * `src/app.py` - (Added auth middleware)
    * `config/settings.json` - (Injected DB credentials)
* **Read-Only Context Files:**
    * `docs/api_spec.md` - (Reference for endpoints)

## 🧠 Acquired Knowledge & Constraints
* [Fact 1: e.g., "The backend uses PostgreSQL 14, not MySQL."]
* [Fact 2: e.g., "Authentication requires a Bearer token, obtained via /login."]
* [Constraint: e.g., "Do not modify the frontend directory; focus only on backend."]

## 🚫 Dead Ends & Resolved Errors
* **Failed:** Tried installing `library-v1`; it conflicts with `core-pkg`. 
    * *Resolution:* Using `library-v2` instead. Do not downgrade.
* **Failed:** Attempted to write to `/root/data/`. 
    * *Resolution:* Permission denied. Working inside `/tmp/data/` for now.

## 🛠️ Recent Action Summary (Last 3-5 Turns)
1.  `read_file(package.json)` -> Identified missing dependency `axios`.
2.  `execute_bash(npm install axios)` -> Success.
3.  `write_file(src/fetcher.js)` -> Drafted initial API call logic.
4.  `execute_bash(node src/fetcher.js)` -> Threw `ReferenceError: process is not defined`.

## 🎯 Immediate Next Step
[Strict, single-action instruction for the agent upon waking up. e.g., "Debug the ReferenceError in src/fetcher.js line 12 by importing the process module, then re-run the file."]