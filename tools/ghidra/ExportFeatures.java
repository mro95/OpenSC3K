// Export compiler-independent function features (strings, constants, calls) and vtables
// for cross-binary name matching (tools/match/match.py).
//
// Usage (headless post-script): ExportFeatures.java <out.tsv|outdir> [vtables.txt|vtdir]
//   Directory arguments resolve to <dir>/<program>.tsv and <dir>/<program>.txt, so one headless
//   run can process many programs.
//   vtables.txt: optional "hexoffset<TAB>bytes<TAB>name" lines giving known vtables (Linux,
//   from __vt_ symbols; offset relative to the lowest load address = Ghidra image base). Without it, vtables are
//   detected heuristically: runs of pointers into executable memory, found in non-executable
//   memory and split where code references them.
//
// Output lines (addresses are absolute hex in Ghidra's address space):
//   # program <name> imagebase <hex> format <fmt>
//   F addr name size ninstr
//   C from to          call to internal function (thunks resolved)
//   X from name        call to external function
//   S from text        referenced string (escaped)
//   K from hex         scalar constant (not an address in this program)
//   R from hex         literal read from read-only data (f4:/f8: raw bits)
//   V addr name slots  vtable; slots comma-separated hex (0 = null, P = pure virtual stub,
//                      ?hex = not a function)
//   VR vtaddr func     function referencing the vtable start
//@category SC3K

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Data;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.scalar.Scalar;
import ghidra.program.model.symbol.ExternalLocation;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.Symbol;

import java.io.BufferedReader;
import java.io.FileReader;
import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import java.util.TreeSet;

public class ExportFeatures extends GhidraScript {
    private Memory mem;
    private PrintWriter out;

    @Override
    protected void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 1) {
            throw new IllegalArgumentException("usage: ExportFeatures.java <out.tsv> [vtables.txt]");
        }
        mem = currentProgram.getMemory();
        String outPath = resolve(args[0], ".tsv");
        String vtPath = args.length > 1 ? resolve(args[1], ".txt") : null;
        int functions = 0, vtables;
        try (PrintWriter w = new PrintWriter(outPath, "UTF-8")) {
            out = w;
            out.printf("# program %s imagebase %s format %s%n", currentProgram.getName(),
                hex(currentProgram.getImageBase()), currentProgram.getExecutableFormat());
            for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
                if (monitor.isCancelled()) {
                    return;
                }
                if (f.isExternal() || f.isThunk()) {
                    continue;
                }
                exportFunction(f);
                functions++;
            }
            vtables = vtPath != null && new java.io.File(vtPath).isFile()
                ? exportKnownVtables(vtPath) : detectVtables();
        }
        println(String.format("%s: %d functions, %d vtables -> %s",
            currentProgram.getName(), functions, vtables, outPath));
    }

    private String resolve(String path, String ext) {
        java.io.File f = new java.io.File(path);
        return f.isDirectory() ? new java.io.File(f, currentProgram.getName() + ext).getPath() : path;
    }

    private void exportFunction(Function f) {
        String entry = hex(f.getEntryPoint());
        int ninstr = 0;
        Set<String> lines = new LinkedHashSet<>();
        for (Instruction insn : currentProgram.getListing().getInstructions(f.getBody(), true)) {
            ninstr++;
            for (Reference ref : insn.getReferencesFrom()) {
                Address to = ref.getToAddress();
                if (ref.getReferenceType().isCall() || ref.getReferenceType().isJump()) {
                    if (!ref.getReferenceType().isCall() && f.getBody().contains(to)) {
                        continue; // intra-function branch
                    }
                    String callee = callTarget(to);
                    callee = callee == null ? null : String.format(callee, entry);
                    if (callee != null) {
                        lines.add(callee);
                    }
                } else if (to.isExternalAddress()) {
                    lines.add("X\t" + entry + "\t" + externalName(to));
                } else if (to.isMemoryAddress()) {
                    dataFeatures(to, entry, lines);
                }
            }
            for (int i = 0; i < insn.getNumOperands(); i++) {
                for (Object o : insn.getOpObjects(i)) {
                    if (o instanceof Scalar) {
                        long v = ((Scalar) o).getUnsignedValue() & 0xffffffffL;
                        // Skip addresses and small signed values (stack/GOT offsets, masks of -n).
                        if (v >= 0x1000 && v < 0xffff0000L && !isAddress(v)) {
                            lines.add("K\t" + entry + "\t" + Long.toHexString(v));
                        }
                    }
                }
            }
        }
        out.printf("F\t%s\t%s\t%d\t%d%n", entry, f.getName(true), f.getBody().getNumAddresses(), ninstr);
        for (String l : lines) {
            out.println(l);
        }
    }

    /** "C"/"X" line for a call/jump target, resolving thunks and import pointers. */
    private String callTarget(Address to) {
        Function g = getFunctionAt(to);
        if (g == null && to.isMemoryAddress()) {
            // PE import call: call dword ptr [IAT slot]
            Data d = getDataAt(to);
            if (d != null && d.isPointer() && d.getValue() instanceof Address) {
                Address p = (Address) d.getValue();
                if (p.isExternalAddress()) {
                    return "X\t%s\t" + externalName(p);
                }
                g = getFunctionAt(p);
            }
        }
        if (g == null) {
            if (to.isExternalAddress()) {
                return "X\t%s\t" + externalName(to);
            }
            return null;
        }
        if (g.isThunk()) {
            g = g.getThunkedFunction(true);
        }
        if (g.isExternal()) {
            return "X\t%s\t" + g.getName(true);
        }
        return "C\t%s\t" + hex(g.getEntryPoint());
    }

    private String externalName(Address a) {
        Symbol s = getSymbolAt(a);
        if (s != null) {
            ExternalLocation loc = currentProgram.getExternalManager().getExternalLocation(s);
            if (loc != null && loc.getOriginalImportedName() != null) {
                return loc.getOriginalImportedName();
            }
            return s.getName(true);
        }
        return "ext:" + a;
    }

    private void dataFeatures(Address to, String entry, Set<String> lines) {
        String s = readString(to);
        if (s != null) {
            lines.add("S\t" + entry + "\t" + escape(s));
            return;
        }
        MemoryBlock b = mem.getBlock(to);
        if (b != null && b.isInitialized() && !b.isExecute() && !b.isWrite()) {
            try {
                int v4 = mem.getInt(to);
                long v8 = mem.getLong(to);
                if (isAddress(v4 & 0xffffffffL)) {
                    return; // a pointer table, not a literal
                }
                if (v4 != 0) {
                    lines.add("R\t" + entry + "\tf4:" + Integer.toHexString(v4));
                }
                if (v8 != 0) {
                    lines.add("R\t" + entry + "\tf8:" + Long.toHexString(v8));
                }
            } catch (Exception e) {
                // unreadable, ignore
            }
        }
    }

    /** ASCII, UTF-16LE (Windows wchar_t) or UTF-32LE (Linux wchar_t) string of >= 4 chars. */
    private String readString(Address a) {
        // GCC inlines short copies of string literals as moves from str+4, str+8, ...:
        // walk back to the start of the string.
        try {
            for (int i = 0; i < 512 && isPrintable(mem.getByte(a.subtract(1)) & 0xff); i++) {
                a = a.subtract(1);
            }
        } catch (Exception e) {
            // start of block
        }
        for (int width : new int[] {1, 2, 4}) {
            StringBuilder sb = new StringBuilder();
            try {
                for (int i = 0; i < 512; i++) {
                    long c;
                    Address p = a.add((long) i * width);
                    if (width == 1) {
                        c = mem.getByte(p) & 0xff;
                    } else if (width == 2) {
                        c = mem.getShort(p) & 0xffff;
                    } else {
                        c = mem.getInt(p) & 0xffffffffL;
                    }
                    if (c == 0) {
                        if (sb.length() >= 4) {
                            return sb.toString();
                        }
                        break;
                    }
                    if (!isPrintable(c)) {
                        break;
                    }
                    sb.append((char) c);
                }
            } catch (Exception e) {
                // ran off the block
            }
        }
        return null;
    }

    private static boolean isPrintable(long c) {
        return c >= 0x20 && c <= 0x7e || c == '\n' || c == '\r' || c == '\t';
    }

    private int exportKnownVtables(String path) throws Exception {
        int n = 0;
        try (BufferedReader r = new BufferedReader(new FileReader(path))) {
            String line;
            while ((line = r.readLine()) != null) {
                String[] p = line.split("\t");
                if (p.length < 3) {
                    continue;
                }
                Address a = currentProgram.getImageBase().add(Long.parseLong(p[0], 16));
                int slots = Integer.parseInt(p[1]) / 4;
                List<String> s = new ArrayList<>();
                for (int i = 0; i < slots; i++) {
                    s.add(slotValue(a.add(4L * i)));
                }
                emitVtable(a, p[2], s);
                n++;
            }
        }
        return n;
    }

    private int detectVtables() throws Exception {
        int n = 0;
        for (MemoryBlock b : mem.getBlocks()) {
            if (!b.isInitialized() || b.isExecute()) {
                continue;
            }
            Address a = b.getStart();
            long words = b.getSize() / 4;
            List<String> run = new ArrayList<>();
            Address runStart = null;
            for (long i = 0; i <= words; i++) {
                Address p = i < words ? a.add(4 * i) : null;
                boolean fp = p != null && isCode(pointerAt(p));
                boolean split = p != null && runStart != null && fp && hasCodeRef(p);
                if (!fp || split) {
                    if (runStart != null && hasCodeRef(runStart)) {
                        emitVtable(runStart, "", run);
                        n++;
                    }
                    run = new ArrayList<>();
                    runStart = null;
                }
                if (fp) {
                    if (runStart == null) {
                        runStart = p;
                    }
                    run.add(slotValue(p));
                }
            }
        }
        return n;
    }

    private Address pointerAt(Address p) {
        try {
            long v = mem.getInt(p) & 0xffffffffL;
            return isAddress(v) ? toAddr(v) : toAddr(0);
        } catch (Exception e) {
            return toAddr(0);
        }
    }

    private String slotValue(Address p) throws Exception {
        long v = mem.getInt(p) & 0xffffffffL;
        if (v == 0) {
            return "0";
        }
        Address t = toAddr(v);
        Function f = getFunctionAt(t);
        if (f != null && f.isThunk() && f.getThunkedFunction(true).isExternal()) {
            return "P"; // import stub: _purecall (MSVC) / __pure_virtual (GCC)
        }
        return (f != null ? "" : "?") + Long.toHexString(v);
    }

    private boolean isCode(Address a) {
        MemoryBlock b = mem.getBlock(a);
        return a.getOffset() != 0 && b != null && b.isExecute();
    }

    private boolean hasCodeRef(Address a) {
        for (Reference r : getReferencesTo(a)) {
            if (getFunctionContaining(r.getFromAddress()) != null) {
                return true;
            }
        }
        return false;
    }

    private void emitVtable(Address a, String name, List<String> slots) {
        out.printf("V\t%s\t%s\t%s%n", hex(a), name, String.join(",", slots));
        Set<String> users = new TreeSet<>();
        for (Reference r : getReferencesTo(a)) {
            Function f = getFunctionContaining(r.getFromAddress());
            if (f != null) {
                users.add(hex(f.getEntryPoint()));
            }
        }
        for (String u : users) {
            out.printf("VR\t%s\t%s%n", hex(a), u);
        }
    }

    private boolean isAddress(long v) {
        try {
            return mem.contains(toAddr(v));
        } catch (Exception e) {
            return false;
        }
    }

    private static String hex(Address a) {
        return Long.toHexString(a.getOffset());
    }

    private static String escape(String s) {
        return s.replace("\\", "\\\\").replace("\t", "\\t").replace("\n", "\\n").replace("\r", "\\r");
    }
}
