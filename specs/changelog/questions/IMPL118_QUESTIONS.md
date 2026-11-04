# IMPL118 questions

1. (non-blocking, session format) specs/data/drawlog/README.md "Trace" lists `ff = −1` rows for presents without a
   Frame_Run calc as "boot, Info_Show". The recorded fullrun_noclip trace also has two such rows for the presents of
   `Device_ClearPresentTwice` inside `Ending_Init` (16 §1.2), i.e. inside a calc: present 121251/121252, calc and poll
   of the running frame, scene_frame counting those presents, then the frame's final row with present 121252. The port
   (`session::on_extra_present`) writes them that way, derived from the column definitions. Please add the in-calc case
   and its numbering to the README.
