-------------------------- MODULE ObserverConsistency --------------------------
(***************************************************************************)
(* INV-C1 / INV-C2 -- observer read-after-write consistency and the        *)
(* retry-on-empty budget (see ../INVARIANTS.md).                           *)
(*                                                                         *)
(* Embers deploys to a validator (gRPC) but reads back via explore-deploy   *)
(* on a SEPARATE observer node. After a deploy finalizes, the observer's    *)
(* state lags before that block is queryable there, so a single-shot        *)
(* read-after-write intermittently returns empty. The client retries on an  *)
(* empty result, with backoff, under a total budget; during each backoff    *)
(* the observer advances toward the finalized head.                        *)
(*                                                                         *)
(* The model reads a value finalized InitialLag blocks ahead of the         *)
(* observer, retrying up to MaxRetries times, and makes checkable:          *)
(*                                                                         *)
(*   - NoFalseRead: a returned value is genuinely visible (obsH >= writeH). *)
(*   - NoPrematureGiveUp (INV-C2): Ok(None) is returned only after the full *)
(*     retry budget is spent (attempts = MaxRetries).                       *)
(*   - BudgetCoversLag: when the budget covers the lag (MaxRetries >=        *)
(*     InitialLag, the regime the code sits in -- TOTAL_READ_BUDGET >=       *)
(*     2 * request_timeout), the read never gives up. The flake config      *)
(*     (MaxRetries < InitialLag) violates this, exhibiting the intermittent *)
(*     empty read that motivated raising the budget.                        *)
(*                                                                         *)
(* Runtime anchor: packages/firefly-client/src/read_node_client.rs:30,      *)
(*   :178-234 (get_data_with_retry, retry only on ReturnValueMissing).      *)
(* Validate:                                                               *)
(*   tlc -config ObserverConsistency.cfg      ObserverConsistency.tla       *)
(*     (no error)                                                          *)
(*   tlc -config ObserverConsistencyFlake.cfg ObserverConsistency.tla       *)
(*     (BudgetCoversLag violated: the intermittent-empty witness)          *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS MaxRetries,      \* how many times the read retries on empty
          InitialLag       \* blocks the finalized value is ahead of the observer

VARIABLES obsH,           \* observer height, relative to the observer's start
          writeH,         \* height at which the read-after-write value finalized
          attempts,       \* retry attempts consumed so far
          phase           \* "reading" | "got" | "gaveup"

vars == <<obsH, writeH, attempts, phase>>

Init == /\ obsH = 0
        /\ writeH = InitialLag
        /\ attempts = 0
        /\ phase = "reading"

(* The value is visible on the observer; the read returns it. *)
ReadValue ==
  /\ phase = "reading"
  /\ obsH >= writeH
  /\ phase' = "got"
  /\ UNCHANGED <<obsH, writeH, attempts>>

(* Not visible yet: retry (INV-C1 -- retry fires only on an empty result).   *)
(* During the backoff the observer advances one block toward the head.       *)
ReadEmpty ==
  /\ phase = "reading"
  /\ obsH < writeH
  /\ attempts < MaxRetries
  /\ attempts' = attempts + 1
  /\ obsH' = obsH + 1
  /\ UNCHANGED <<writeH, phase>>

(* The budget is spent and the value is still not visible: return Ok(None). *)
GaveUp ==
  /\ phase = "reading"
  /\ obsH < writeH
  /\ attempts = MaxRetries
  /\ phase' = "gaveup"
  /\ UNCHANGED <<obsH, writeH, attempts>>

Next == ReadValue \/ ReadEmpty \/ GaveUp

Spec == Init /\ [][Next]_vars

TypeOK == /\ obsH \in 0..InitialLag
          /\ writeH = InitialLag
          /\ attempts \in 0..MaxRetries
          /\ phase \in {"reading", "got", "gaveup"}

(* A returned value is genuinely visible -- never a stale or absent read. *)
NoFalseRead == (phase = "got") => (obsH >= writeH)

(* INV-C2: a give-up surfaces only after the full retry budget is spent. *)
NoPrematureGiveUp == (phase = "gaveup") => (attempts = MaxRetries)

(* When the budget covers the lag the read never gives up. Holds in the code's *)
(* regime (MaxRetries >= InitialLag); the flake config falsifies it.           *)
BudgetCoversLag == phase # "gaveup"
===============================================================================
