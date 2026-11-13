LATEX      ?= pdflatex
LATEXFLAGS ?= -interaction=nonstopmode -halt-on-error -file-line-error
SOURCES    := manual.tex $(wildcard sections/*.tex) $(wildcard appendices/*.tex)
PDF        := ../manual.pdf

.PHONY: all clean

all: $(PDF)

$(PDF): $(SOURCES)
	$(LATEX) $(LATEXFLAGS) manual.tex
	$(LATEX) $(LATEXFLAGS) manual.tex
	mv -f manual.pdf $(PDF)

clean:
	rm -f $(PDF) manual.pdf manual.aux manual.log manual.out manual.toc \
	      manual.lof manual.lot
