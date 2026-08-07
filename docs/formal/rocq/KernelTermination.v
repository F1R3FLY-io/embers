(* ==========================================================================
   KernelTermination.v — INV-K14: the durable kernel's hot segment is bounded, so
   the run loop terminates on a valid workflow.

   ProgressBoundary.v proves that a valid workflow's residual ("hot") graph — the
   edges that are NOT progress boundaries — is acyclic. A finite acyclic graph admits
   a topological rank: a `rank : node -> nat` that strictly increases along every hot
   edge. This file consumes that rank (the kernel's termination measure) and proves
   that a token following hot edges:

     - visits strictly-increasing ranks, hence never repeats a node (a simple path),
     - so its path length is at most the number of nodes (the "hot segment <= |N_H|"
       bound), and
     - is at most rank(end) - rank(start) + 1 (an explicit step budget parameterized
       by the definition's rank range).

   Between progress boundaries the kernel therefore makes bounded progress, and since
   every cycle crosses a progress boundary (ProgressBoundary.v), the run loop cannot
   diverge. The rank/strict-increase pair is a section parameter — the constructive
   witness of residual acyclicity — so nothing here is an axiom.
   ========================================================================== *)

From Stdlib Require Import List.
From Stdlib Require Import Lia.
Import ListNotations.

Section KernelTermination.
Context {A : Type}.
Variable Hot : A -> A -> Prop.          (* the residual / "hot" edge relation *)
Variable rank : A -> nat.               (* topological rank (termination measure) *)
Hypothesis Hrank : forall x y, Hot x y -> rank x < rank y.

(* A hot walk from x to z, recorded as the list of nodes it visits (head x, last z). *)
Inductive Walk : A -> A -> list A -> Prop :=
| w_end  : forall x,       Walk x x (x :: nil)
| w_more : forall x y z l, Hot x y -> Walk y z l -> Walk x z (x :: l).

(* Every node on a walk has rank at least the walk's start rank. *)
Lemma walk_rank_lb : forall x z l, Walk x z l -> forall v, In v l -> rank x <= rank v.
Proof.
  intros x z l W. induction W as [x | x y z l Hxy W IH]; intros v Hin.
  - destruct Hin as [<- | []]. lia.
  - destruct Hin as [<- | Hin].
    + lia.
    + apply IH in Hin. apply Hrank in Hxy. lia.
Qed.

(* The end node is on the walk. *)
Lemma walk_end_in : forall x z l, Walk x z l -> In z l.
Proof.
  intros x z l W. induction W as [x | x y z l Hxy W IH].
  - left; reflexivity.
  - right; exact IH.
Qed.

(* Hence the end rank dominates the start rank. *)
Lemma walk_end_rank : forall x z l, Walk x z l -> rank x <= rank z.
Proof.
  intros x z l W. apply (walk_rank_lb x z l W z). apply (walk_end_in x z l W).
Qed.

(* Strictly-increasing ranks mean no node repeats: the path is simple. *)
Lemma walk_nodup : forall x z l, Walk x z l -> NoDup l.
Proof.
  intros x z l W. induction W as [x | x y z l Hxy W IH].
  - constructor; [ intros [] | constructor ].
  - constructor.
    + intro Hin. apply (walk_rank_lb y z l W) in Hin. apply Hrank in Hxy. lia.
    + exact IH.
Qed.

(* INV-K14 (space): a hot segment visits at most |nodes| nodes. *)
Theorem hot_segment_bounded :
  forall x z l verts, Walk x z l -> incl l verts -> length l <= length verts.
Proof.
  intros x z l verts W Hincl.
  apply NoDup_incl_length; [ apply (walk_nodup x z l W) | exact Hincl ].
Qed.

(* INV-K14 (measure): an explicit step budget from the rank range. *)
Theorem kernel_step_bound :
  forall x z l, Walk x z l -> length l <= S (rank z - rank x).
Proof.
  intros x z l W. induction W as [x | x y z l Hxy W IH].
  - simpl. lia.
  - simpl. pose proof (walk_end_rank y z l W). apply Hrank in Hxy. lia.
Qed.

End KernelTermination.
