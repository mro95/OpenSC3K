// Apply names ported from the Loki Linux build (tools/match/names/<program>.tsv) to the
// current program: Class::method becomes a namespaced function name, and the full GCC
// signature plus match provenance go in the plate comment. Functions with user-defined
// names are left alone.
//
// Usage (headless post-script): ApplyNames.java <names.tsv|namesdir>
//@category SC3K

import ghidra.app.script.GhidraScript;
import ghidra.app.util.NamespaceUtils;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.CodeUnit;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.Namespace;
import ghidra.program.model.symbol.SourceType;
import ghidra.program.model.symbol.SymbolUtilities;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileReader;
import java.util.ArrayList;
import java.util.List;

public class ApplyNames extends GhidraScript {
    @Override
    protected void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length != 1) {
            throw new IllegalArgumentException("usage: ApplyNames.java <names.tsv|namesdir>");
        }
        File f = new File(args[0]);
        if (f.isDirectory()) {
            f = new File(f, currentProgram.getName() + ".tsv");
        }
        if (!f.isFile()) {
            println(currentProgram.getName() + ": no names file, skipped");
            return;
        }
        int applied = 0, skipped = 0, failed = 0;
        try (BufferedReader r = new BufferedReader(new FileReader(f))) {
            r.readLine(); // header: rva demangled mangled source method
            String line;
            while ((line = r.readLine()) != null) {
                String[] p = line.split("\t");
                Address a = currentProgram.getImageBase().add(Long.parseLong(p[0], 16));
                Function fn = getFunctionAt(a);
                if (fn == null || fn.getSymbol().getSource() == SourceType.USER_DEFINED) {
                    skipped++;
                    continue;
                }
                try {
                    List<String> path = qualifiedName(p[1]);
                    String name = SymbolUtilities.replaceInvalidChars(path.remove(path.size() - 1), true);
                    Namespace ns = currentProgram.getGlobalNamespace();
                    if (!path.isEmpty()) {
                        List<String> clean = new ArrayList<>();
                        for (String s : path) {
                            clean.add(SymbolUtilities.replaceInvalidChars(s, true));
                        }
                        ns = NamespaceUtils.createNamespaceHierarchy(String.join("::", clean), null,
                            currentProgram, SourceType.IMPORTED);
                    }
                    fn.getSymbol().setNameAndNamespace(name, ns, SourceType.IMPORTED);
                    fn.setComment(null);
                    currentProgram.getListing().setComment(a, CodeUnit.PLATE_COMMENT,
                        p[1] + "\nLoki " + p[3] + ": " + p[2] + "\nmatched by " + p[4]);
                    applied++;
                } catch (Exception e) {
                    printerr(p[0] + " " + p[1] + ": " + e.getMessage());
                    failed++;
                }
            }
        }
        println(String.format("%s: applied %d, skipped %d, failed %d", currentProgram.getName(),
            applied, skipped, failed));
    }

    /** "ns::cA<int, x>::Get(int) const" -> [ns, cA<int, x>, Get]. */
    static List<String> qualifiedName(String demangled) {
        List<String> parts = new ArrayList<>();
        int depth = 0, start = 0;
        for (int i = 0; i < demangled.length(); i++) {
            char c = demangled.charAt(i);
            if (c == '<') {
                depth++;
            } else if (c == '>') {
                depth--;
            } else if (c == '(' && depth == 0 && !demangled.substring(0, i).endsWith("operator")) {
                parts.add(demangled.substring(start, i));
                return parts;
            } else if (c == ':' && depth == 0 && i + 1 < demangled.length() && demangled.charAt(i + 1) == ':') {
                parts.add(demangled.substring(start, i));
                start = i + 2;
                i++;
            }
        }
        parts.add(demangled.substring(start));
        return parts;
    }
}
