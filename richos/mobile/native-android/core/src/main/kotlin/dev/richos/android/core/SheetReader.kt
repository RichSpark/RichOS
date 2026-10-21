package dev.richos.android.core

import org.w3c.dom.Document
import org.w3c.dom.Element
import java.io.File
import java.math.BigDecimal
import java.math.RoundingMode
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import java.util.Locale
import java.util.zip.ZipFile
import javax.xml.parsers.DocumentBuilderFactory

/**
 * A spreadsheet's first sheet, as the cells' own text (round 36 "Phone: opened", the comps): what
 * the phone's viewer shows for an XLSX or CSV Rich attached. Read from the kept file itself, never
 * drawn: Android has no spreadsheet viewer of its own, so the app reads the workbook's XML (an XLSX
 * is a zip of it) and shows the values as the file's number formats write them.
 */
data class SheetTable(
    /** The sheet's name in the workbook ("Comparable deals"); null for a CSV. */
    val title: String?,
    /** Row by row, every row as wide as the widest; empty cells are "". */
    val rows: List<List<String>>,
    /** More rows or columns than the viewer shows; Share hands the whole file on. */
    val cut: Boolean = false,
)

object SheetReader {
    /** What the viewer shows at most: a phone screen's worth many times over, never the whole of a huge sheet. */
    const val MAX_ROWS = 500
    const val MAX_COLUMNS = 40
    /** One part of the zip, uncompressed; a larger one is refused rather than read into memory. */
    private const val MAX_PART = 24L * 1024 * 1024

    /** The kinds the viewer reads itself. */
    fun reads(ext: String): Boolean = ext.uppercase(Locale.ROOT) in setOf("XLSX", "CSV")

    /** The first sheet of the file at [path], or null when it cannot be read as one. */
    fun read(path: String, ext: String): SheetTable? = runCatching {
        when (ext.uppercase(Locale.ROOT)) {
            "XLSX" -> xlsx(File(path))
            "CSV" -> csv(File(path))
            else -> null
        }
    }.getOrNull()

    // --- XLSX (Office Open XML, ECMA-376 part 1 §18) ------------------------------------------

    private const val REL_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"

    private fun xlsx(file: File): SheetTable? = ZipFile(file).use { zip ->
        fun doc(name: String): Document? {
            val entry = zip.getEntry(name) ?: return null
            if (entry.size > MAX_PART) throw IllegalStateException("$name is too large")
            return zip.getInputStream(entry).use { input ->
                val bytes = input.readNBytesCapped(MAX_PART)
                parser().parse(bytes.inputStream())
            }
        }
        val workbook = doc("xl/workbook.xml") ?: return null
        val sheet = workbook.all("sheet").firstOrNull()
        val title = sheet?.getAttribute("name")?.takeIf { it.isNotBlank() }
        val rid = sheet?.getAttributeNS(REL_NS, "id")?.takeIf { it.isNotBlank() }
        val target = rid?.let { id ->
            doc("xl/_rels/workbook.xml.rels")?.all("Relationship")?.firstOrNull { it.getAttribute("Id") == id }?.getAttribute("Target")
        }?.let { t -> if (t.startsWith("/")) t.removePrefix("/") else "xl/" + t.removePrefix("./") }
        val sheetDoc = (target?.let { doc(it) } ?: doc("xl/worksheets/sheet1.xml")) ?: return null
        val shared = doc("xl/sharedStrings.xml")?.all("si")?.map { si -> si.all("t").filter { it.notPhonetic() }.joinToString("") { it.textContent } }.orEmpty()
        val formats = doc("xl/styles.xml")?.let { styles(it) }.orEmpty()

        val cells = sortedMapOf<Int, MutableMap<Int, String>>()
        var cut = false
        var nextRow = 1
        for (row in sheetDoc.all("row")) {
            val r = row.getAttribute("r").toIntOrNull() ?: nextRow
            nextRow = r + 1
            if (r > MAX_ROWS) { cut = true; break }
            var nextCol = 0
            for (c in row.children("c")) {
                val col = column(c.getAttribute("r")) ?: nextCol
                nextCol = col + 1
                if (col >= MAX_COLUMNS) { cut = true; continue }
                val text = value(c, shared, formats)
                if (text.isNotEmpty()) cells.getOrPut(r) { mutableMapOf() }[col] = text
            }
        }
        table(title, cells, cut)
    }

    /** One cell's text: a shared or inline string, a boolean, an error, or a number in its format. */
    private fun value(c: Element, shared: List<String>, formats: List<String?>): String {
        val v = c.children("v").firstOrNull()?.textContent
        return when (c.getAttribute("t")) {
            "s" -> v?.trim()?.toIntOrNull()?.let { shared.getOrNull(it) } ?: ""
            "inlineStr" -> c.children("is").firstOrNull()?.all("t")?.filter { it.notPhonetic() }?.joinToString("") { it.textContent } ?: ""
            "b" -> if (v?.trim() == "1") "TRUE" else "FALSE"
            "str", "e" -> v ?: ""
            else -> v?.let { number(it.trim(), formats.getOrNull(c.getAttribute("s").toIntOrNull() ?: 0)) } ?: ""
        }
    }

    /** Each cell style's number format code, by style index (`cellXfs`). */
    private fun styles(doc: Document): List<String?> {
        val custom = doc.all("numFmt").associate { it.getAttribute("numFmtId").toIntOrNull() to it.getAttribute("formatCode") }
        val xfs = doc.all("cellXfs").firstOrNull()?.children("xf").orEmpty()
        return xfs.map { xf ->
            val id = xf.getAttribute("numFmtId").toIntOrNull() ?: 0
            custom[id] ?: BUILT_IN[id]
        }
    }

    /** The built-in number formats a workbook names by id only (ECMA-376 §18.8.30). */
    private val BUILT_IN = mapOf(
        1 to "0", 2 to "0.00", 3 to "#,##0", 4 to "#,##0.00", 9 to "0%", 10 to "0.00%", 11 to "0.00E+00",
        14 to "yyyy-mm-dd", 15 to "yyyy-mm-dd", 16 to "yyyy-mm-dd", 17 to "yyyy-mm-dd",
        18 to "h:mm", 19 to "h:mm", 20 to "h:mm", 21 to "h:mm", 22 to "yyyy-mm-dd h:mm",
        37 to "#,##0", 38 to "#,##0", 39 to "#,##0.00", 40 to "#,##0.00", 45 to "h:mm", 46 to "h:mm", 47 to "h:mm",
    )

    /** A number as its format code writes it: percent, grouping, decimals, currency, a date or time. */
    internal fun number(raw: String, code: String?): String {
        val n = raw.toDoubleOrNull() ?: return raw
        val f = code?.substringBefore(';')?.let(::plain)?.takeIf { it.isNotBlank() && !it.equals("General", true) && it != "@" }
            ?: return general(n)
        val lower = f.lowercase(Locale.ROOT)
        if (!lower.contains('0') && !lower.contains('#') && (lower.contains('y') || lower.contains('d') || lower.contains('h') || lower.contains('m'))) {
            return date(n, lower)
        }
        if (lower.contains('e') && lower.contains("e+")) return String.format(Locale.US, "%.2E", n)
        val decimals = Regex("\\.([0#]+)").find(f)?.groupValues?.get(1)?.length ?: 0
        val percent = f.contains('%')
        val value = if (percent) n * 100 else n
        val pattern = (if (f.contains(',')) "#,##0" else "0") + (if (decimals > 0) "." + "0".repeat(decimals) else "")
        val body = DecimalFormat(pattern, DecimalFormatSymbols(Locale.US)).apply { roundingMode = RoundingMode.HALF_UP }.format(kotlin.math.abs(value))
        val currency = Regex("[$€£¥]").find(f)?.value ?: ""
        val sign = if (value < 0 && body.any { it in '1'..'9' }) "-" else ""
        return sign + currency + body + (if (percent) "%" else "")
    }

    /** The code without what only paints it: colors and conditions in brackets (a `[$€-x]` currency kept), quotes, padding. */
    private fun plain(code: String): String =
        code.replace(Regex("\\[\\$([^\\]-]*)[^\\]]*\\]")) { it.groupValues[1] }
            .replace(Regex("\\[[^\\]]*\\]"), "")
            .replace(Regex("_."), "").replace(Regex("\\*."), "")
            .replace("\\", "").replace("\"", "")

    /** A spreadsheet serial (days since 1899-12-30) as a date, a time, or both. */
    private fun date(serial: Double, code: String): String {
        val day = kotlin.math.floor(serial).toLong()
        val seconds = Math.round((serial - day) * 86_400)
        val at = LocalDateTime.of(LocalDate.of(1899, 12, 30).plusDays(day), java.time.LocalTime.MIDNIGHT).plusSeconds(seconds)
        val hasDate = code.contains('y') || code.contains('d')
        val hasTime = code.contains('h') || code.contains('s')
        return when {
            hasDate && hasTime -> at.format(DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm"))
            hasTime -> at.format(DateTimeFormatter.ofPattern("HH:mm"))
            else -> at.toLocalDate().format(DateTimeFormatter.ISO_LOCAL_DATE)
        }
    }

    /** A number with no format: whole numbers whole, others to at most ten significant digits. */
    private fun general(n: Double): String {
        if (n == kotlin.math.floor(n) && kotlin.math.abs(n) < 1e15) return n.toLong().toString()
        return BigDecimal(n).round(java.math.MathContext(10)).stripTrailingZeros().toPlainString()
    }

    /** "B12" -> 1; null for a reference without letters. */
    private fun column(ref: String): Int? {
        val letters = ref.takeWhile { it.isLetter() }.uppercase(Locale.ROOT)
        if (letters.isEmpty()) return null
        return letters.fold(0) { acc, ch -> acc * 26 + (ch - 'A' + 1) } - 1
    }

    // --- CSV (RFC 4180) --------------------------------------------------------------------------

    private fun csv(file: File): SheetTable {
        val text = file.inputStream().use { it.readNBytesCapped(MAX_PART) }.toString(Charsets.UTF_8).removePrefix("﻿")
        val cells = sortedMapOf<Int, MutableMap<Int, String>>()
        var row = 1
        var col = 0
        var cut = false
        val field = StringBuilder()
        var quoted = false
        var i = 0
        fun endField() {
            if (col < MAX_COLUMNS) { if (field.isNotEmpty()) cells.getOrPut(row) { mutableMapOf() }[col] = field.toString() } else cut = true
            field.setLength(0)
            col++
        }
        while (i < text.length) {
            val ch = text[i]
            when {
                quoted && ch == '"' && i + 1 < text.length && text[i + 1] == '"' -> { field.append('"'); i++ }
                ch == '"' && (quoted || field.isEmpty()) -> quoted = !quoted
                !quoted && ch == ',' -> endField()
                !quoted && (ch == '\n' || ch == '\r') -> {
                    if (ch == '\r' && i + 1 < text.length && text[i + 1] == '\n') i++
                    endField()
                    row++
                    col = 0
                    if (row > MAX_ROWS) { cut = i + 1 < text.length; break }
                }
                else -> field.append(ch)
            }
            i++
        }
        if (row <= MAX_ROWS && (field.isNotEmpty() || col > 0)) endField()
        return table(null, cells, cut)
    }

    // --- shared ------------------------------------------------------------------------------------

    /** The grid, every row as wide as the widest, empty rows inside kept, empty ones at the end dropped. */
    private fun table(title: String?, cells: Map<Int, Map<Int, String>>, cut: Boolean): SheetTable {
        if (cells.isEmpty()) return SheetTable(title, emptyList(), cut)
        val first = cells.keys.min()
        val last = cells.keys.max()
        val width = cells.values.maxOf { it.keys.max() } + 1
        val rows = (first..last).map { r -> List(width) { c -> cells[r]?.get(c) ?: "" } }
        return SheetTable(title, rows, cut)
    }

    private fun parser() = DocumentBuilderFactory.newInstance().apply {
        isNamespaceAware = true
        isExpandEntityReferences = false
        // No document type, so no entity can reach outside the file (where the platform's parser knows the switch).
        runCatching { setFeature("http://apache.org/xml/features/disallow-doctype-decl", true) }
        runCatching { setFeature("http://xml.org/sax/features/external-general-entities", false) }
    }.newDocumentBuilder()

    private fun Document.all(local: String): List<Element> = getElementsByTagNameNS("*", local).let { l -> List(l.length) { l.item(it) as Element } }
    private fun Element.all(local: String): List<Element> = getElementsByTagNameNS("*", local).let { l -> List(l.length) { l.item(it) as Element } }
    private fun Element.children(local: String): List<Element> {
        val out = mutableListOf<Element>()
        var n = firstChild
        while (n != null) {
            if (n is Element && (n.localName ?: n.nodeName.substringAfter(':')) == local) out += n
            n = n.nextSibling
        }
        return out
    }

    /** A `t` that is the cell's text, not a phonetic guide (`rPh`) above it. */
    private fun Element.notPhonetic(): Boolean = (parentNode as? Element)?.let { (it.localName ?: it.nodeName) != "rPh" } ?: true

    private fun java.io.InputStream.readNBytesCapped(cap: Long): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        val buf = ByteArray(64 * 1024)
        var total = 0L
        while (true) {
            val n = read(buf)
            if (n < 0) break
            total += n
            if (total > cap) throw IllegalStateException("too large")
            out.write(buf, 0, n)
        }
        return out.toByteArray()
    }
}
