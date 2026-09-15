// observer_retry.pm — quantitative reliability of read-after-write retry (INV-C1..C3).
//
// After a write finalizes on the validator, the observer becomes visible only after a
// catch-up lag. A read that chases a just-finalized write retries ONLY on the empty
// result (ReturnValueMissing), with capped exponential backoff under a 150 s budget
// (firefly-client read_node_client.rs). This DTMC abstracts each retry attempt as a
// Bernoulli catch-up trial: with probability `pcatch` the observer has caught up by
// that attempt (the read then returns the value and succeeds); otherwise the read is
// empty and the attempt is retried. The read "flakes" only if every attempt within the
// budget precedes catch-up.
//
// pcatch = 0.5 is deliberately PESSIMISTIC: real observer lag is a few seconds against a
// 150 s budget, so the true per-attempt catch-up probability is far higher. maxAttempts
// = 10 reflects the tail of the capped-exponential schedule within the budget.

dtmc

const double pcatch = 0.5;     // pessimistic per-attempt observer catch-up probability
const int    maxAttempts = 10; // read attempts that fit within TOTAL_READ_BUDGET = 150 s

module observer_read
  attempt : [0..maxAttempts] init 0;
  visible : bool init false;   // the observer has caught up to the finalized write
  done    : bool init false;
  succ    : bool init false;

  // an attempt: catch-up (success) with pcatch, else retry (empty result)
  [try] (!done) & (attempt < maxAttempts) & (!visible) ->
        pcatch     : (visible'=true) & (succ'=true) & (done'=true)
      + (1-pcatch) : (attempt'=attempt+1);

  // budget exhausted without catch-up -> give up (the flake case)
  [giveup] (!done) & (attempt = maxAttempts) & (!visible) -> (done'=true);
endmodule

label "flake"   = done & !succ;
label "success" = done &  succ;
