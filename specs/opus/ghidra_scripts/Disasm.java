// Disassemble functions (addr|name) or ranges addr:len into GH_OUT
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class Disasm extends GhidraScript {
    @Override
    public void run() throws Exception {
        java.io.PrintWriter pw = new java.io.PrintWriter(new java.io.FileWriter(System.getenv("GH_OUT"), true), true);
        Listing l = currentProgram.getListing();
        for (String a : getScriptArgs()) {
            AddressSetView body;
            if (a.contains(":")) {
                String[] p = a.split(":");
                Address s = toAddr(p[0]);
                body = new AddressSet(s, s.add(Long.decode(p[1]) - 1));
            } else {
                Function f = getFunctionContaining(toAddr(a));
                if (f == null) { pw.println("NOT FOUND " + a); continue; }
                pw.println("===== " + f.getName() + " @ " + f.getEntryPoint());
                body = f.getBody();
            }
            for (Instruction i : l.getInstructions(body, true)) {
                String lab = "";
                ghidra.program.model.symbol.Symbol s = getSymbolAt(i.getAddress());
                if (s != null) lab = s.getName() + ":";
                pw.println(i.getAddress() + "  " + lab + "\t" + i.toString());
            }
        }
        pw.close();
    }
}
