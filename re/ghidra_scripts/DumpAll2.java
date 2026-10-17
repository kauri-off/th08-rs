// Decompile all functions (current names) + callgraph + global xrefs. Args: outdir
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.util.*;

public class DumpAll2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        String dir = getScriptArgs()[0];
        FunctionManager fm = currentProgram.getFunctionManager();
        Memory mem = currentProgram.getMemory();
        DecompInterface d = new DecompInterface();
        d.openProgram(currentProgram);
        PrintWriter c = new PrintWriter(new FileWriter(dir + "/all_decomp.c.tmp"));
        PrintWriter cg = new PrintWriter(new FileWriter(dir + "/callgraph.tsv.tmp"));
        cg.println("addr\tname\tcallers\tcallees");
        TreeMap<Address, TreeSet<String>[]> glob = new TreeMap<>();
        int n = 0, unnamed = 0, failed = 0;
        for (Function f : fm.getFunctions(true)) {
            n++;
            if (f.getName().startsWith("FUN_")) unnamed++;
            DecompileResults r = d.decompileFunction(f, 60, monitor);
            c.println("===== " + f.getName() + " @ " + f.getEntryPoint());
            if (r.decompileCompleted()) c.println(r.getDecompiledFunction().getC());
            else { c.println("FAILED: " + r.getErrorMessage()); failed++; }
            TreeSet<String> callers = new TreeSet<>(), callees = new TreeSet<>();
            for (Function g : f.getCallingFunctions(monitor)) callers.add(g.getName());
            for (Function g : f.getCalledFunctions(monitor)) callees.add(g.getName());
            cg.println(f.getEntryPoint() + "\t" + f.getName() + "\t" + String.join(",", callers) + "\t" + String.join(",", callees));
            for (Address a : f.getBody().getAddresses(true)) {
                for (Reference ref : currentProgram.getReferenceManager().getReferencesFrom(a)) {
                    Address to = ref.getToAddress();
                    if (!to.isMemoryAddress() || ref.getReferenceType().isFlow()) continue;
                    MemoryBlock b = mem.getBlock(to);
                    if (b == null || b.isExecute()) continue;
                    RefType t = ref.getReferenceType();
                    int k = t.isWrite() ? (t.isRead() ? 2 : 1) : (t.isRead() ? 0 : 3);
                    @SuppressWarnings("unchecked")
                    TreeSet<String>[] e = glob.computeIfAbsent(to, x -> new TreeSet[]{new TreeSet<>(), new TreeSet<>(), new TreeSet<>(), new TreeSet<>()});
                    e[k].add(f.getName());
                }
            }
        }
        c.close();
        cg.close();
        PrintWriter gx = new PrintWriter(new FileWriter(dir + "/globals_xref.tsv.tmp"));
        gx.println("addr\tname\treaders\twriters\treadwrite\taddress_taken");
        for (Map.Entry<Address, TreeSet<String>[]> e : glob.entrySet()) {
            Symbol s = currentProgram.getSymbolTable().getPrimarySymbol(e.getKey());
            String nm = s == null ? "" : s.getName();
            TreeSet<String>[] v = e.getValue();
            gx.println(e.getKey() + "\t" + nm + "\t" + String.join(",", v[0]) + "\t" + String.join(",", v[1]) + "\t" + String.join(",", v[2]) + "\t" + String.join(",", v[3]));
        }
        gx.close();
        PrintWriter pw = new PrintWriter(new FileWriter(System.getenv("GH_OUT"), true), true);
        pw.println("functions " + n + ", unnamed (FUN_) " + unnamed + ", decompile failed " + failed + ", globals " + glob.size());
        pw.close();
    }
}
