(* ==========================================================================
   ProgressBoundary.v — INV-K6: the progress-boundary characterization of
   terminating workflow graphs.

   The workflow validator (workflow.rs validate_cycles) requires every cycle in a
   workflow graph to contain at least one "progress" edge — a bound or wait edge
   that guarantees forward progress each time round. This file proves that this
   condition is EQUIVALENT to the residual graph (the workflow with all progress
   edges removed) being acyclic. That equivalence is what makes the kernel's
   hot-segment traversal terminate (see KernelTermination.v): the hot segment
   follows only non-progress edges, and it is finite exactly when the residual is
   acyclic.

   Graphs are edge relations over an arbitrary node type; progress edges are a
   decidable subset (via a boolean pb reflecting P). No axioms, no admits — the
   graph, its progress set, and the decidability reflection are section parameters,
   discharged into the theorem statement when the section closes.
   ========================================================================== *)

From Stdlib Require Import Bool.

Section ProgressBoundary.
Context {A : Type}.
Variable G  : A -> A -> Prop.          (* the workflow edge relation *)
Variable P  : A -> A -> Prop.          (* progress edges (bound / wait) *)
Variable pb : A -> A -> bool.          (* decision procedure for P *)
Hypothesis pb_ok : forall x y, pb x y = true <-> P x y.

(* The residual graph: the workflow with every progress edge removed. *)
Definition R (x y : A) : Prop := G x y /\ ~ P x y.

(* A nonempty walk over an edge relation E. *)
Inductive Walk (E : A -> A -> Prop) : A -> A -> Prop :=
| w_one  : forall x y,   E x y -> Walk E x y
| w_more : forall x y z, E x y -> Walk E y z -> Walk E x z.

Definition Acyclic (E : A -> A -> Prop) : Prop := forall x, ~ Walk E x x.

(* A nonempty walk over G, tracking (as a bool) whether it used a progress edge. *)
Inductive GWalk : A -> A -> bool -> Prop :=
| gw_one  : forall x y,     G x y -> GWalk x y (pb x y)
| gw_more : forall x y z b, G x y -> GWalk y z b -> GWalk x z (pb x y || b).

(* Every G-cycle uses at least one progress edge. *)
Definition EveryCycleCrossesProgress : Prop :=
  forall x b, GWalk x x b -> b = true.

(* pb reflects P, so its negation reflects ~P. *)
Lemma pb_false_notP : forall x y, pb x y = false -> ~ P x y.
Proof.
  intros x y Hf HP. apply pb_ok in HP. rewrite HP in Hf. discriminate.
Qed.

Lemma notP_pb_false : forall x y, ~ P x y -> pb x y = false.
Proof.
  intros x y HnP. destruct (pb x y) eqn:E; [ | reflexivity ].
  exfalso. apply HnP. apply pb_ok. exact E.
Qed.

(* A progress-free G-walk is exactly a walk in the residual graph. *)
Lemma gwalk_false_is_R : forall x y b, GWalk x y b -> b = false -> Walk R x y.
Proof.
  intros x y b H. induction H as [x y HG | x y z b HG HW IH]; intro Hb.
  - (* gw_one: the tracked bool is pb x y, so pb x y = false *)
    apply w_one. unfold R. split; [ exact HG | apply pb_false_notP; exact Hb ].
  - (* gw_more: pb x y || b = false, so both are false *)
    apply orb_false_iff in Hb. destruct Hb as [Hpb Hb0].
    apply w_more with (y := y).
    + unfold R. split; [ exact HG | apply pb_false_notP; exact Hpb ].
    + apply IH; exact Hb0.
Qed.

Lemma R_is_gwalk_false : forall x y, Walk R x y -> GWalk x y false.
Proof.
  intros x y H. induction H as [x y HR | x y z HR HW IH].
  - unfold R in HR. destruct HR as [HG HnP].
    rewrite <- (notP_pb_false x y HnP). apply gw_one. exact HG.
  - unfold R in HR. destruct HR as [HG HnP].
    replace false with (pb x y || false)
      by (rewrite (notP_pb_false x y HnP); reflexivity).
    apply gw_more; [ exact HG | exact IH ].
Qed.

(* THEOREM (INV-K6): every workflow cycle crosses a progress boundary IFF the
   residual graph is acyclic. *)
Theorem progress_boundary_iff_residual_acyclic :
  EveryCycleCrossesProgress <-> Acyclic R.
Proof.
  unfold EveryCycleCrossesProgress, Acyclic. split.
  - (* -> : a residual cycle would be a progress-free G-cycle *)
    intros ECCP x Hcyc.
    apply R_is_gwalk_false in Hcyc.
    apply ECCP in Hcyc. discriminate.
  - (* <- : a progress-free G-cycle would be a residual cycle *)
    intros Hac x b Hgw. destruct b.
    + reflexivity.
    + exfalso. apply (Hac x). apply (gwalk_false_is_R x x false Hgw). reflexivity.
Qed.

End ProgressBoundary.
