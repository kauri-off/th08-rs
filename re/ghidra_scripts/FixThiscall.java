// Set __thiscall on functions that read ECX before writing it (and aren't fastcall/thiscall already)
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.lang.*;
import ghidra.program.model.symbol.SourceType;

public class FixThiscall extends GhidraScript {
    @Override
    public void run() throws Exception {
        java.io.PrintWriter pw = new java.io.PrintWriter(new java.io.FileWriter(System.getenv("GH_OUT"), true), true);
        Register ecx = currentProgram.getRegister("ECX");
        int n = 0, m2 = 0;
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            String cc = f.getCallingConventionName();
            if (cc != null && cc.contains("fastcall")) continue;
            boolean reads = false;
            int k = 0;
            for (Instruction i : currentProgram.getListing().getInstructions(f.getBody(), true)) {
                if (++k > 12) break;
                boolean r = false, w = false;
                for (Object o : i.getInputObjects()) if (o instanceof Register && ((Register) o).contains(ecx)) r = true;
                for (Object o : i.getResultObjects()) if (o instanceof Register && ((Register) o).contains(ecx)) w = true;
                String m = i.getMnemonicString();
                if (m.equals("XOR") && i.toString().contains("ECX,ECX")) r = false;
                if (m.equals("PUSH")) r = false;
                if (r) { reads = true; break; }
                if (w) break;
                if (m.equals("CALL") || m.equals("RET") || m.startsWith("J")) break;
            }
            boolean isThis = cc != null && cc.contains("thiscall");
            if (reads && !isThis) { f.setCallingConvention("__thiscall"); n++; }
            if (!reads && isThis) { f.setCallingConvention("__stdcall"); m2++; }
        }
        pw.println("thiscall set on " + n + ", reverted " + m2);
        pw.close();
    }
}
