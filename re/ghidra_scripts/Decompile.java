// Decompile functions: args = address or name, multiple allowed
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;

public class Decompile extends GhidraScript {
    java.io.PrintWriter pw;
    void out(String s) {
        try {
            if (pw == null) pw = new java.io.PrintWriter(new java.io.FileWriter(System.getenv("GH_OUT"), true), true);
            pw.println(s);
        } catch (Exception e) { println(s); }
    }
    @Override
    public void run() throws Exception {
        DecompInterface d = new DecompInterface();
        d.openProgram(currentProgram);
        for (String a : getScriptArgs()) {
            Function f = resolve(a);
            if (f == null) { out("NOT FOUND: " + a); continue; }
            DecompileResults r = d.decompileFunction(f, 120, monitor);
            out("===== " + f.getName() + " @ " + f.getEntryPoint());
            out(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "FAILED: " + r.getErrorMessage());
        }
    }
    Function resolve(String a) {
        try {
            Address ad = toAddr(a);
            if (ad != null) {
                Function f = getFunctionContaining(ad);
                if (f != null) return f;
            }
        } catch (Exception e) {}
        for (Function f : currentProgram.getFunctionManager().getFunctions(true))
            if (f.getName().equals(a)) return f;
        return null;
    }
}
