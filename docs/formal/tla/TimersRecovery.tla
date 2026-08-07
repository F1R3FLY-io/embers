------------------------- MODULE TimersRecovery -------------------------
(***************************************************************************)
(* Durable timer / recovery scheduler (INV-T1, T3, T4), in discrete time.  *)
(*                                                                         *)
(* A suspended run carries a deadline D relative to its suspend instant. A  *)
(* scheduler polls every P and, on finding the run due (now >= suspendT+D), *)
(* resumes it within a bound B. An approval wait times out at Tap but is    *)
(* HELD, not abandoned, so a late decision is still delivered (commit       *)
(* 4a8b791). Time is the integer variable `now`; urgent actions are forced  *)
(* by disabling Tick at their deadlines (UrgentPending). Because every      *)
(* timing constant is an integer, this discrete model is faithful to the    *)
(* mechanism the UPPAAL timed automaton in ../uppaal/timers_recovery.xml    *)
(* describes; it is the machine-checked artifact on hosts whose UPPAAL       *)
(* license (as here) does not cover verifyta.                              *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS D, P, B, Tap, MaxTime

VARIABLES now, node, suspendT, sched, resumeT, pcLast, appr, early
vars == <<now, node, suspendT, sched, resumeT, pcLast, appr, early>>

Init ==
  /\ now = 0
  /\ node = "idle0"
  /\ suspendT = 0
  /\ sched = "idle"
  /\ resumeT = 0
  /\ pcLast = 0
  /\ appr = "waiting"
  /\ early = FALSE

(* The run suspends once; it may do so at any tick in [0, P]. *)
Suspend ==
  /\ node = "idle0"
  /\ node' = "suspended"
  /\ suspendT' = now
  /\ UNCHANGED <<now, sched, resumeT, pcLast, appr, early>>

(* A scheduler poll fires exactly at pcLast+P and finds nothing due. *)
PollIdle ==
  /\ sched = "idle"
  /\ now = pcLast + P
  /\ ~(node = "suspended" /\ now >= suspendT + D)
  /\ pcLast' = now
  /\ UNCHANGED <<now, node, suspendT, sched, resumeT, appr, early>>

(* A poll that finds the run due: enter Resuming, to resume within B. *)
PollDue ==
  /\ sched = "idle"
  /\ now = pcLast + P
  /\ node = "suspended"
  /\ now >= suspendT + D
  /\ sched' = "resuming"
  /\ resumeT' = now
  /\ pcLast' = now
  /\ UNCHANGED <<now, node, suspendT, appr, early>>

(* The scheduler issues the resume within B of deciding. `early` records any
   resume that ever preceded the deadline; the honest scheduler resumes only when
   due, so it must stay FALSE. *)
Resume ==
  /\ sched = "resuming"
  /\ node = "suspended"
  /\ now <= resumeT + B
  /\ node' = "resumed"
  /\ early' = (early \/ (now < suspendT + D))
  /\ sched' = "idle"
  /\ UNCHANGED <<now, suspendT, resumeT, pcLast, appr>>

(* Approval times out at Tap, but is HELD rather than abandoned. *)
Timeout ==
  /\ appr = "waiting"
  /\ now = Tap
  /\ appr' = "held"
  /\ UNCHANGED <<now, node, suspendT, sched, resumeT, pcLast, early>>

(* A decision may arrive at any time: on time from waiting, or late from held. *)
Decide ==
  /\ appr \in {"waiting", "held"}
  /\ appr' = "decided"
  /\ UNCHANGED <<now, node, suspendT, sched, resumeT, pcLast, early>>

(* Time may not pass through a deadline at which an urgent action must fire. *)
UrgentPending ==
  \/ (node = "idle0"     /\ now = P)
  \/ (sched = "idle"     /\ now = pcLast + P)
  \/ (sched = "resuming" /\ now = resumeT + B)
  \/ (appr = "waiting"   /\ now = Tap)

Tick ==
  /\ now < MaxTime
  /\ ~UrgentPending
  /\ now' = now + 1
  /\ UNCHANGED <<node, suspendT, sched, resumeT, pcLast, appr, early>>

Next == Suspend \/ PollIdle \/ PollDue \/ Resume \/ Timeout \/ Decide \/ Tick

Fairness ==
  /\ WF_vars(Suspend) /\ WF_vars(PollIdle) /\ WF_vars(PollDue)
  /\ WF_vars(Resume)  /\ WF_vars(Decide)   /\ WF_vars(Tick)

Spec == Init /\ [][Next]_vars /\ Fairness

TypeOK ==
  /\ now \in 0..MaxTime
  /\ node  \in {"idle0", "suspended", "resumed"}
  /\ sched \in {"idle", "resuming"}
  /\ appr  \in {"waiting", "held", "decided"}
  /\ early \in BOOLEAN

(* INV-T1: the honest scheduler never resumes before the deadline. *)
NoEarlyResume == early = FALSE

(* INV-T3: a suspended run is resumed within D + P + B of suspending. *)
ResumeWithinBound == (node = "suspended") => (now - suspendT <= D + P + B)

(* INV-T3 / durable-yield: a suspended run is always eventually resumed. *)
AutoResume == <>(node = "resumed")

(* INV-T4: a held approval always eventually reaches a delivered decision. *)
LateDecisionDelivered == [](appr = "held" => <>(appr = "decided"))

(* Every approval is eventually decided (on time or late). *)
ApprovalDecided == <>(appr = "decided")
=============================================================================
