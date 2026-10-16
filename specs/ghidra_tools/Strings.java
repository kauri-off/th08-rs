// Dump defined strings with xref functions; optional arg = substring filter
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;

public class Strings extends GhidraScript {
    java.io.PrintWriter pw;
    void out(String s) {
        try {
            if (pw == null) pw = new java.io.PrintWriter(new java.io.FileWriter(System.getenv("GH_OUT"), true), true);
            pw.println(s);
        } catch (Exception e) { println(s); }
    }
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        String filt = args.length > 0 ? args[0] : null;
        for (Data d : currentProgram.getListing().getDefinedData(true)) {
            if (!d.hasStringValue()) continue;
            String v = String.valueOf(d.getValue());
            if (filt != null && !v.contains(filt)) continue;
            StringBuilder sb = new StringBuilder();
            for (Reference r : getReferencesTo(d.getAddress())) {
                Function f = getFunctionContaining(r.getFromAddress());
                if (f != null) sb.append(" ").append(f.getName()).append("@").append(f.getEntryPoint());
            }
            out(d.getAddress() + " \"" + v.replace("\n", "\\n") + "\"" + sb);
        }
    }
}
