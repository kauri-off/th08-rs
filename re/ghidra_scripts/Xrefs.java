// List references to each address/name arg, with containing function
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;

public class Xrefs extends GhidraScript {
    java.io.PrintWriter pw;
    void out(String s) {
        try {
            if (pw == null) pw = new java.io.PrintWriter(new java.io.FileWriter(System.getenv("GH_OUT"), true), true);
            pw.println(s);
        } catch (Exception e) { println(s); }
    }
    @Override
    public void run() throws Exception {
        for (String a : getScriptArgs()) {
            Address ad = null;
            for (Symbol s : currentProgram.getSymbolTable().getSymbols(a)) { ad = s.getAddress(); break; }
            if (ad == null) ad = toAddr(a);
            out("===== xrefs to " + a + " (" + ad + ")");
            for (Reference r : getReferencesTo(ad)) {
                Function f = getFunctionContaining(r.getFromAddress());
                out(r.getFromAddress() + " " + r.getReferenceType() + " in " + (f == null ? "?" : f.getName() + "@" + f.getEntryPoint()));
            }
        }
    }
}
