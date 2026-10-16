// Decompile every function into GH_OUT
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;

public class DumpAll extends GhidraScript {
    @Override
    public void run() throws Exception {
        java.io.PrintWriter pw = new java.io.PrintWriter(new java.io.FileWriter(System.getenv("GH_OUT"), true), true);
        DecompInterface d = new DecompInterface();
        d.openProgram(currentProgram);
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            DecompileResults r = d.decompileFunction(f, 60, monitor);
            pw.println("===== " + f.getName() + " @ " + f.getEntryPoint());
            pw.println(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "FAILED: " + r.getErrorMessage());
        }
        pw.close();
    }
}
