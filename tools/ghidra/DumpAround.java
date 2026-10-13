// Headless Ghidra script: dumps a function index, all strings with the functions that
// reference them, and decompiled C for every function into re/out/.
// Output is derived from the original exe and must never be committed (re/ is git-ignored).
//
// Usage (via tools/ghidra/analyze.sh):
//   analyzeHeadless <proj_dir> TS2 -import toy2.exe -postScript DumpAround.java <out_dir>
//@category TS2

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.data.StringDataInstance;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.DataIterator;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.address.Address;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.symbol.Reference;

import java.io.File;
import java.io.PrintWriter;
import java.util.TreeSet;

public class DumpAround extends GhidraScript {
    @Override
    protected void run() throws Exception {
        String[] args = getScriptArgs();
        File out = new File(args.length > 0 ? args[0] : "re_out");
        out.mkdirs();
        int made = createMissedFunctions();
        println("Created " + made + " functions in unanalysed code");

        try (PrintWriter w = new PrintWriter(new File(out, "functions.tsv"), "UTF-8")) {
            FunctionIterator it = currentProgram.getFunctionManager().getFunctions(true);
            while (it.hasNext()) {
                Function f = it.next();
                w.printf("%s\t%s\t%d%n", f.getEntryPoint(), f.getName(), f.getBody().getNumAddresses());
            }
        }

        try (PrintWriter w = new PrintWriter(new File(out, "strings.tsv"), "UTF-8")) {
            DataIterator di = currentProgram.getListing().getDefinedData(true);
            while (di.hasNext()) {
                Data d = di.next();
                if (!StringDataInstance.isString(d)) continue;
                String s = StringDataInstance.getStringDataInstance(d).getStringValue();
                if (s == null) continue;
                TreeSet<String> users = new TreeSet<>();
                for (Reference r : getReferencesTo(d.getAddress())) {
                    Function f = getFunctionContaining(r.getFromAddress());
                    if (f != null) users.add(f.getEntryPoint() + ":" + f.getName());
                }
                w.printf("%s\t%s\t%s%n", d.getAddress(), s.replace("\t", "\\t").replace("\n", "\\n").replace("\r", "\\r"), String.join(",", users));
            }
        }

        DecompInterface dec = new DecompInterface();
        dec.openProgram(currentProgram);
        File cdir = new File(out, "c");
        cdir.mkdirs();
        FunctionIterator it = currentProgram.getFunctionManager().getFunctions(true);
        int n = 0;
        while (it.hasNext() && !monitor.isCancelled()) {
            Function f = it.next();
            DecompileResults r = dec.decompileFunction(f, 60, monitor);
            if (r == null || r.getDecompiledFunction() == null) continue;
            try (PrintWriter w = new PrintWriter(new File(cdir, f.getEntryPoint() + ".c"), "UTF-8")) {
                w.println("// " + f.getName() + " @ " + f.getEntryPoint());
                w.print(r.getDecompiledFunction().getC());
            }
            n++;
        }
        println("Decompiled " + n + " functions to " + cdir);
    }

    /**
     * Code reached only through jump tables or pointer tables is often left undefined by
     * auto-analysis. Walk .text: wherever bytes aren't inside a function, skip padding
     * (0x90/0xCC), disassemble and create a function there; repeat until nothing changes.
     */
    private int createMissedFunctions() throws Exception {
        MemoryBlock text = currentProgram.getMemory().getBlock(".text");
        if (text == null) return 0;
        int made = 0;
        for (int pass = 0; pass < 8; pass++) {
            int before = made;
            Address a = text.getStart();
            while (a.compareTo(text.getEnd()) < 0 && !monitor.isCancelled()) {
                Function f = getFunctionContaining(a);
                if (f != null) {
                    a = f.getBody().getMaxAddress().add(1);
                    continue;
                }
                int b = getByte(a) & 0xFF;
                if (b == 0x90 || b == 0xCC || b == 0x00) {
                    a = a.add(1);
                    continue;
                }
                Instruction ins = getInstructionAt(a);
                if (ins == null) {
                    disassemble(a);
                    ins = getInstructionAt(a);
                }
                if (ins != null && createFunction(a, null) != null) {
                    made++;
                    continue;
                }
                a = a.add(1);
            }
            if (made == before) break;
        }
        return made;
    }
}
