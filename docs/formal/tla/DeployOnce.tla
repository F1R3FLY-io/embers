------------------------------ MODULE DeployOnce ------------------------------
(***************************************************************************)
(* INV-C4 -- deploy-once / at-most-once landing (see ../INVARIANTS.md).     *)
(*                                                                         *)
(* A client submits a deploy and never retries an ambiguous outcome,       *)
(* because a lost response could mean the deploy already landed on chain.  *)
(* This model compares the deployed no-retry policy against a hypothetical  *)
(* retry policy over one logical deploy intent, and makes two statements    *)
(* machine-checkable:                                                      *)
(*                                                                         *)
(*   - Under "noRetry", at most one copy of the deploy ever lands          *)
(*     (AtMostOnceLanding holds).                                          *)
(*   - Under "retry", a timed-out-but-landed submission followed by a      *)
(*     resubmission lands a second copy -- TLC reports AtMostOnceLanding    *)
(*     violated. That counterexample is the falsification witness for why   *)
(*     the code never retries (write_node_client.rs:179-180).              *)
(*                                                                         *)
(* The model also encodes the honest NON-guarantee that a client-visible   *)
(* error does not imply the deploy did not land: TimedOutButLanded reaches  *)
(* a landed state with a client error. Semantics are at-most-once, not      *)
(* exactly-once.                                                           *)
(*                                                                         *)
(* Runtime anchor: packages/firefly-client/src/write_node_client.rs:179-180.*)
(* Validate:                                                               *)
(*   tlc -config DeployOnce.cfg      DeployOnce.tla   (no error)           *)
(*   tlc -config DeployOnceRetry.cfg DeployOnce.tla   (AtMostOnceLanding    *)
(*                                                     violated: the        *)
(*                                                     duplicate-landing    *)
(*                                                     witness)             *)
(***************************************************************************)
EXTENDS Naturals

CONSTANT Policy            \* "noRetry" or "retry"

MaxRounds == 2             \* bound the submission rounds for finite checking

VARIABLES rounds,          \* how many times the client has submitted
          landed,          \* how many copies of the deploy have landed on chain
          done             \* the client has stopped (no further submissions)

vars == <<rounds, landed, done>>

Init == rounds = 0 /\ landed = 0 /\ done = FALSE

(* A clean ack: the deploy lands and the client learns it succeeded, so it     *)
(* stops regardless of policy.                                                 *)
Ack ==
  /\ ~done
  /\ rounds < MaxRounds
  /\ rounds' = rounds + 1
  /\ landed' = landed + 1
  /\ done'   = TRUE

(* A lost response: nothing landed, the client sees an error. Under retry it    *)
(* resubmits (a fresh round); under no-retry it stops.                          *)
Lost ==
  /\ ~done
  /\ rounds < MaxRounds
  /\ rounds' = rounds + 1
  /\ landed' = landed
  /\ done'   = (Policy = "noRetry")

(* The ambiguous outcome: the deploy landed but the client's call timed out and *)
(* it sees an error. Under retry it resubmits -- and a following Ack lands a     *)
(* SECOND copy. Under no-retry it stops.                                        *)
TimedOutButLanded ==
  /\ ~done
  /\ rounds < MaxRounds
  /\ rounds' = rounds + 1
  /\ landed' = landed + 1
  /\ done'   = (Policy = "noRetry")

(* Out of rounds: the client stops.                                            *)
Stop ==
  /\ ~done
  /\ rounds = MaxRounds
  /\ done'  = TRUE
  /\ UNCHANGED <<rounds, landed>>

Next == Ack \/ Lost \/ TimedOutButLanded \/ Stop

Spec == Init /\ [][Next]_vars

TypeOK == /\ rounds \in 0..MaxRounds
          /\ landed \in 0..MaxRounds
          /\ done   \in BOOLEAN

(* Safety: at most one copy of one deploy intent ever lands. *)
AtMostOnceLanding == landed <= 1
===============================================================================
