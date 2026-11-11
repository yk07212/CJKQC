LATEX      ?= pdflatex
LATEXFLAGS ?= -interaction=nonstopmode -halt-on-error -file-line-error
SOURCES    := manual.tex $(wildcard sections/*.tex) $(wildcard appendices/*.tex)

.PHONY: all clean

all: manual.pdf

manual.pdf: $(SOURCES)
	$(LATEX) $(LATEXFLAGS) manual.tex
	$(LATEX) $(LATEXFLAGS) manual.tex

clean:
	rm -f manual.pdf manual.aux manual.log manual.out manual.toc \
	      manual.lof manual.lot
