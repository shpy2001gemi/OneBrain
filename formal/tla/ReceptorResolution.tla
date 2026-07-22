------------------------ MODULE ReceptorResolution -------------------------
EXTENDS Naturals

VARIABLES proposed, materialized, adopted
vars == <<proposed, materialized, adopted>>

Init == /\ proposed = FALSE
        /\ materialized = FALSE
        /\ adopted = FALSE

Propose == /\ ~proposed
           /\ proposed' = TRUE
           /\ UNCHANGED <<materialized, adopted>>

Materialize == /\ proposed
               /\ ~materialized
               /\ materialized' = TRUE
               /\ UNCHANGED <<proposed, adopted>>

Adopt == /\ materialized
         /\ ~adopted
         /\ adopted' = TRUE
         /\ UNCHANGED <<proposed, materialized>>

Next == Propose \/ Materialize \/ Adopt
Spec == Init /\ [][Next]_vars

MaterializationNeedsProposal == materialized => proposed
ActiveNeedsMaterialization == adopted => materialized
ProposalIsNotAdoption == (~materialized) => (~adopted)

=============================================================================
