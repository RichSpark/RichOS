package dev.richos.android.core

import java.io.File
import java.nio.file.Files
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * The phone's viewer reads a spreadsheet Rich attached from the file itself (round 36 "Phone:
 * opened", the comps; review rv-20261021T100557Z-21af5639-503f: the XLSX showed drawn bars).
 * The workbook here is written the way Excel and Numbers write one: shared strings, a named first
 * sheet behind a relationship, styles with currency and percent formats.
 */
class SheetReaderTest {
    private fun xlsx(dir: File, parts: Map<String, String>): File {
        val file = File(dir, "Acme-comparables.xlsx")
        ZipOutputStream(file.outputStream()).use { zip ->
            for ((name, text) in parts) {
                zip.putNextEntry(ZipEntry(name))
                zip.write(text.toByteArray())
                zip.closeEntry()
            }
        }
        return file
    }

    private val main = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    private val rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"

    private fun comps(dir: File) = xlsx(dir, mapOf(
        "xl/workbook.xml" to """<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="$main" xmlns:r="$rel"><sheets><sheet name="Comparable deals" sheetId="1" r:id="rId3"/></sheets></workbook>""",
        "xl/_rels/workbook.xml.rels" to """<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId3" Type="$rel/worksheet" Target="worksheets/deals.xml"/></Relationships>""",
        "xl/sharedStrings.xml" to """<sst xmlns="$main"><si><t>Customer</t></si><si><t>Year</t></si><si><t>List</t></si><si><t>Discount</t></si><si><r><t>Acme </t></r><r><t>Corp. (ask)</t></r></si><si><t>Bellweather</t></si></sst>""",
        "xl/styles.xml" to """<styleSheet xmlns="$main"><numFmts count="1"><numFmt numFmtId="164" formatCode="&quot;$&quot;#,##0"/></numFmts><cellXfs count="4"><xf numFmtId="0"/><xf numFmtId="164"/><xf numFmtId="10"/><xf numFmtId="14"/></cellXfs></styleSheet>""",
        "xl/worksheets/deals.xml" to """<worksheet xmlns="$main"><sheetData>
            <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="s"><v>2</v></c><c r="D1" t="s"><v>3</v></c></row>
            <row r="2"><c r="A2" t="s"><v>4</v></c><c r="B2"><v>2026</v></c><c r="C2" s="1"><v>240000</v></c><c r="D2" s="2"><v>0.08</v></c></row>
            <row r="4"><c r="A4" t="s"><v>5</v></c><c r="B4" t="inlineStr"><is><t>2026</t></is></c><c r="C4" s="1"><v>198000</v></c><c r="E4" s="3"><v>46316</v></c></row>
            </sheetData></worksheet>""",
    ))

    @Test fun theFirstSheetIsReadFromTheFileWithItsFormats() {
        val dir = Files.createTempDirectory("sheet").toFile()
        try {
            val table = SheetReader.read(comps(dir).path, "XLSX")!!
            assertEquals("Comparable deals", table.title)
            assertEquals(listOf("Customer", "Year", "List", "Discount", ""), table.rows[0])
            assertEquals(listOf("Acme Corp. (ask)", "2026", "$240,000", "8.00%", ""), table.rows[1])
            assertEquals(listOf("", "", "", "", ""), table.rows[2], "an empty row inside the sheet stays")
            assertEquals(listOf("Bellweather", "2026", "$198,000", "", "2026-10-21"), table.rows[3])
            assertEquals(false, table.cut)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test fun numbersAreWrittenAsTheirFormatsSay() {
        assertEquals("1,234.50", SheetReader.number("1234.5", "#,##0.00"))
        assertEquals("-$1,234", SheetReader.number("-1234.4", "\"$\"#,##0;[Red]\\(\"$\"#,##0\\)"))
        assertEquals("€12", SheetReader.number("12", "[$€-407]#,##0"))
        assertEquals("5%", SheetReader.number("0.05", "0%"))
        assertEquals("0.3333333333", SheetReader.number("0.333333333333", null))
        assertEquals("42", SheetReader.number("42", "General"))
        assertEquals("text", SheetReader.number("text", "0.00"))
    }

    @Test fun aCsvIsReadWithItsQuotes() {
        val dir = Files.createTempDirectory("sheet").toFile()
        try {
            val file = File(dir, "comps.csv")
            file.writeText("﻿Customer,List\r\n\"Granite & Co., Ltd\",\"\$289,000\"\r\n\"He said \"\"firm\"\"\",3\n")
            val table = SheetReader.read(file.path, "csv")!!
            assertNull(table.title)
            assertEquals(listOf(listOf("Customer", "List"), listOf("Granite & Co., Ltd", "\$289,000"), listOf("He said \"firm\"", "3")), table.rows)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test fun whatIsNotASpreadsheetIsNotReadAsOne() {
        val dir = Files.createTempDirectory("sheet").toFile()
        try {
            val file = File(dir, "broken.xlsx").apply { writeText("%PDF-1.4 not a zip") }
            assertNull(SheetReader.read(file.path, "XLSX"))
            assertNull(SheetReader.read(file.path, "DOCX"))
            assertTrue(SheetReader.reads("xlsx") && SheetReader.reads("CSV") && !SheetReader.reads("PDF"))
        } finally {
            dir.deleteRecursively()
        }
    }
}
