(* ==========================================================================
   MergeJournal.v — INV-D8: both audit sequences survive the journal merge.

   get_run.rs:merge_event_sequences reconciles a run's two ordered audit
   sequences (the durable checkpoint journal and the on-chain ledger) using
   repeated ledger events as anchors. Its output is emitted block by block: for
   each aligned anchor it appends the durable segment since the previous anchor,
   then the on-chain segment since the previous anchor, then the shared anchor
   once; after the last anchor it appends both remaining tails. The doc comment's
   guarantee is INV-D8: "Both inputs remain subsequences of the result."

   This file mechanizes that block structure and proves, with NO axioms / admits,
   that BOTH reconstructed inputs are subsequences of the merge. The anchor value
   is what makes the on-chain side a subsequence too: in the merge it appears once
   (as the durable element), and it equals the on-chain element it matched, so it
   serves as on-chain[c_i] as well as durable[d_i].

   Faithfulness: the worked Example below is exactly the algorithm's output on
   durable = [A;X;C], on_chain = [B;X;D] (anchor X at durable idx 1 / on-chain
   idx 1): the Rust code yields [A;B;X;C;D], and reconstructD / reconstructC
   recover the two inputs verbatim.
   ========================================================================== *)

From Stdlib Require Import List.
Import ListNotations.

Section Merge.
Context {A : Type}.

(* l1 is a subsequence of l2: drop-preserving-order embedding. *)
Inductive Subseq : list A -> list A -> Prop :=
| ss_nil  : forall l, Subseq [] l
| ss_skip : forall x l1 l2, Subseq l1 l2 -> Subseq l1 (x :: l2)
| ss_take : forall x l1 l2, Subseq l1 l2 -> Subseq (x :: l1) (x :: l2).

Lemma Subseq_refl : forall l, Subseq l l.
Proof. induction l as [| x l IH]; [ apply ss_nil | apply ss_take, IH ]. Qed.

(* Prepending the SAME prefix to both sides preserves subsequence. *)
Lemma Subseq_app_head : forall s a b, Subseq a b -> Subseq (s ++ a) (s ++ b).
Proof.
  induction s as [| x s IH]; intros a b H; simpl.
  - exact H.
  - apply ss_take, IH, H.
Qed.

(* Skipping an arbitrary prefix of the target preserves subsequence. *)
Lemma Subseq_skip_app : forall s a b, Subseq a b -> Subseq a (s ++ b).
Proof.
  induction s as [| x s IH]; intros a b H; simpl.
  - exact H.
  - apply ss_skip, IH, H.
Qed.

(* A list is a subsequence of itself followed by any suffix. *)
Lemma Subseq_app_r : forall a b, Subseq a (a ++ b).
Proof.
  induction a as [| x a IH]; intros b; simpl.
  - apply ss_nil.
  - apply ss_take, IH.
Qed.

(* A merge is a list of anchor blocks (durable segment, on-chain segment, anchor)
   plus the two trailing tails. This mirrors merge_event_sequences' emission. *)
Definition Block : Type := (list A * list A * A).

Fixpoint anchorMerge (blocks : list Block) (tailD tailC : list A) : list A :=
  match blocks with
  | [] => tailD ++ tailC
  | (sd, sc, a) :: rest => sd ++ sc ++ [a] ++ anchorMerge rest tailD tailC
  end.

(* The durable input, reconstructed from the same blocks: each block contributes
   its durable segment then the anchor; then the durable tail. *)
Fixpoint reconstructD (blocks : list Block) (tailD : list A) : list A :=
  match blocks with
  | [] => tailD
  | (sd, _, a) :: rest => sd ++ [a] ++ reconstructD rest tailD
  end.

(* The on-chain input, reconstructed likewise: the anchor stands in for the
   on-chain element it matched. *)
Fixpoint reconstructC (blocks : list Block) (tailC : list A) : list A :=
  match blocks with
  | [] => tailC
  | (_, sc, a) :: rest => sc ++ [a] ++ reconstructC rest tailC
  end.

(* INV-D8, durable side: the durable input is a subsequence of the merge. *)
Theorem durable_is_subseq :
  forall blocks tailD tailC,
    Subseq (reconstructD blocks tailD) (anchorMerge blocks tailD tailC).
Proof.
  induction blocks as [| [[sd sc] a] rest IH]; intros tailD tailC; simpl.
  - (* [] : reconstructD = tailD, anchorMerge = tailD ++ tailC *)
    apply Subseq_app_r.
  - (* block: sd ++ [a] ++ RD   vs   sd ++ sc ++ [a] ++ AM *)
    apply Subseq_app_head.          (* drop the shared sd prefix *)
    apply Subseq_skip_app.          (* skip sc on the right *)
    apply (Subseq_app_head [a]).    (* keep the anchor *)
    apply IH.
Qed.

(* INV-D8, on-chain side: the on-chain input is a subsequence of the merge. *)
Theorem on_chain_is_subseq :
  forall blocks tailD tailC,
    Subseq (reconstructC blocks tailC) (anchorMerge blocks tailD tailC).
Proof.
  induction blocks as [| [[sd sc] a] rest IH]; intros tailD tailC; simpl.
  - (* [] : reconstructC = tailC, anchorMerge = tailD ++ tailC *)
    apply Subseq_skip_app, Subseq_refl.
  - (* block: sc ++ [a] ++ RC   vs   sd ++ sc ++ [a] ++ AM *)
    apply Subseq_skip_app.          (* skip sd on the right *)
    apply (Subseq_app_head sc).     (* drop the shared sc prefix *)
    apply ss_take.                  (* keep the anchor a *)
    apply IH.
Qed.

End Merge.

(* Faithfulness: the algorithm's output on durable=[A;X;C], on_chain=[B;X;D]. *)
Example algo_output_matches :
  anchorMerge [([1], [2], 24)] [3] [4] = [1; 2; 24; 3; 4].
Proof. reflexivity. Qed.

Example durable_reconstructed :
  reconstructD [([1], [2], 24)] [3] = [1; 24; 3].   (* = durable input *)
Proof. reflexivity. Qed.

Example on_chain_reconstructed :
  reconstructC [([1], [2], 24)] [4] = [2; 24; 4].   (* = on-chain input *)
Proof. reflexivity. Qed.
