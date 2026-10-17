// Apply symbols.csv (addr,kind,name,signature,confidence,note) to the program
import ghidra.app.script.GhidraScript;
import ghidra.app.util.cparser.C.CParserUtils;
import ghidra.app.cmd.function.ApplyFunctionSignatureCmd;
import ghidra.program.model.data.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;

public class ApplySymbols extends GhidraScript {
    java.io.PrintWriter pw;
    void out(String s) {
        try {
            if (pw == null) pw = new java.io.PrintWriter(new java.io.FileWriter(System.getenv("GH_OUT"), true), true);
            pw.println(s);
        } catch (Exception e) { println(s); }
    }
    @Override
    public void run() throws Exception {
        String path = getScriptArgs()[0];
        int ok = 0, bad = 0;
        for (String line : Files.readAllLines(Paths.get(path))) {
            line = line.trim();
            if (line.isEmpty() || line.startsWith("#") || line.startsWith("addr,")) continue;
            String[] c = line.split(",", 6);
            try {
                Address ad = toAddr(c[0].trim());
                String kind = c[1].trim(), name = c[2].trim();
                String sig = c.length > 3 ? c[3].trim() : "";
                String note = c.length > 5 ? c[5].trim() : "";
                if (kind.equals("func")) {
                    Function f = getFunctionAt(ad);
                    if (f == null) f = createFunction(ad, name);
                    f.setName(name, SourceType.USER_DEFINED);
                    if (!sig.isEmpty()) {
                        FunctionDefinitionDataType def = CParserUtils.parseSignature((ghidra.app.services.DataTypeManagerService) null, currentProgram, sig);
                        if (def != null) new ApplyFunctionSignatureCmd(ad, def, SourceType.USER_DEFINED).applyTo(currentProgram, monitor);
                    }
                    if (!note.isEmpty()) f.setComment(note);
                } else {
                    Symbol s = getSymbolAt(ad);
                    if (s != null && s.getSource() != SourceType.DEFAULT) s.setName(name, SourceType.USER_DEFINED);
                    else createLabel(ad, name, true, SourceType.USER_DEFINED);
                    if (!note.isEmpty()) setPlateComment(ad, note);
                }
                ok++;
            } catch (Exception e) {
                out("FAIL: " + line + " -> " + e.getMessage());
                bad++;
            }
        }
        out("applied " + ok + ", failed " + bad);
    }
}
