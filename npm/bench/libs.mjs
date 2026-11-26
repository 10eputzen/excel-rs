// One adapter per library: read a sheet into rows of values, write rows to xlsx.
import { createRequire } from 'node:module';
import { Readable } from 'node:stream';

const require = createRequire(import.meta.url);

export const LIBS = {
  'excel-rs': {
    label: 'excel-rs',
    load: () => {
      const x = require('../dist/node/index.js');
      return {
        readXlsx: (buf) => x.readSheet(buf).rows,
        readCsv: (buf) => x.readSheet(buf).rows,
        writeXlsx: (rows) => x.writeXlsx([{ name: 'S', columns: [], rows }]),
      };
    },
  },
  sheetjs: {
    label: 'SheetJS 0.20.3',
    load: () => sheetjs(require('xlsx')),
  },
  exceljs: {
    label: 'exceljs 4.4.0',
    load: () => {
      const ExcelJS = require('exceljs');
      const rowsOf = (ws) => {
        const rows = [];
        ws.eachRow({ includeEmpty: true }, (row) => rows.push(row.values.slice(1)));
        return rows;
      };
      return {
        readXlsx: async (buf) => {
          const wb = new ExcelJS.Workbook();
          await wb.xlsx.load(buf);
          return rowsOf(wb.worksheets[0]);
        },
        readCsv: async (buf) => {
          const wb = new ExcelJS.Workbook();
          const ws = await wb.csv.read(Readable.from([buf]), { parserOptions: { delimiter: ';' } });
          return rowsOf(ws);
        },
        writeXlsx: async (rows) => {
          const wb = new ExcelJS.Workbook();
          const ws = wb.addWorksheet('S');
          for (const r of rows) ws.addRow(r);
          return Buffer.from(await wb.xlsx.writeBuffer());
        },
      };
    },
  },
  'read-excel-file': {
    label: 'read-excel-file 9.3.20',
    load: async () => {
      const { default: readXlsxFile } = await import('read-excel-file/node');
      // Version 9 returns every sheet: [{ sheet, data }].
      return { readXlsx: async (buf) => (await readXlsxFile(buf))[0].data };
    },
  },
};

function sheetjs(XLSX) {
  const rows = (wb) => XLSX.utils.sheet_to_json(wb.Sheets[wb.SheetNames[0]], { header: 1, raw: true, defval: null });
  return {
    readXlsx: (buf) => rows(XLSX.read(buf, { cellDates: true })),
    readCsv: (buf) => rows(XLSX.read(buf.toString('utf8'), { type: 'string', FS: ';' })),
    writeXlsx: (data) => {
      const wb = XLSX.utils.book_new();
      XLSX.utils.book_append_sheet(wb, XLSX.utils.aoa_to_sheet(data), 'S');
      return XLSX.write(wb, { type: 'buffer', bookType: 'xlsx' });
    },
  };
}
