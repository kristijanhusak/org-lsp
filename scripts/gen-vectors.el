;;; gen-vectors.el --- Reference values for org-core's date helpers  -*- lexical-binding: t; -*-

;; Usage: TZ=UTC emacs --batch -Q -l scripts/gen-vectors.el OUTPUT-DIR
;;
;; Writes tab-separated files of inputs and the results Org computes for
;; them. `server/org-core/tests/vectors.rs' checks the Rust ports against
;; these. The random inputs are seeded, so the output is reproducible.

(require 'org)
(require 'org-agenda)
(require 'cl-lib)

(random "org-lsp")

(defun v-pick (list) (nth (random (length list)) list))

(defun v-date ()
  (format "%04d-%02d-%02d"
          (+ 2019 (random 9)) (1+ (random 12))
          (v-pick '(1 2 10 15 28 28 29 30 31))))

(defun v-valid-date ()
  (let ((d (calendar-gregorian-from-absolute (+ 737000 (random 3500)))))
    (format "%04d-%02d-%02d" (nth 2 d) (nth 0 d) (nth 1 d))))

(defun v-write (file rows)
  (with-temp-file file
    (dolist (row (nreverse rows))
      (insert (mapconcat (lambda (x) (if (stringp x) x (format "%S" x))) row "\t") "\n"))))

(defun v-closest-date ()
  (let (rows)
    (dotimes (_ 4000)
      (let* ((start (concat (v-valid-date)
                            (v-pick '("" " Mon" " Mon 08:00" " Mon 23:30" " 5:00"))
                            " " (v-pick '("+" "++" ".+"))
                            (number-to-string (v-pick '(0 1 1 1 2 3 5 7 12 24 30 40 100)))
                            (v-pick '("h" "d" "d" "w" "w" "m" "m" "y" "y"))
                            (v-pick '("" "" " -3d"))))
             (current (+ (time-to-days (org-time-string-to-time start)) (- (random 1500) 300)))
             (prefer (v-pick '(past future nil))))
        (push (list start current (or prefer "nearest") (org-closest-date start current prefer)) rows)))
    rows))

(defun v-to-day ()
  (let (rows)
    (dotimes (_ 1500)
      (let ((s (concat (v-pick '("" "<" "x ")) (v-date)
                       (v-pick '("" " Mon" " Sat" "  Mon" " mån" "Mon"))
                       (v-pick '("" "" " 08:00" " 8:00" " 23:59" " 24:00" " 25:30" " 99:99" "  7:05" " 08:0" " 1:2:3"))
                       (v-pick '("" " +1w" " -2d" ">")))))
        (push (list s (time-to-days (org-time-string-to-time s))
                    (let ((p (org-parse-time-string s t))) (list (nth 2 p) (nth 1 p))))
              rows)))
    rows))

(defun v-wdays ()
  (let (rows)
    (dotimes (_ 1500)
      (let* ((s (concat (v-valid-date) " Mon"
                        (v-pick '("" " 10:00" " +1w" " .+2d"))
                        (v-pick '("" " " "  "))
                        (v-pick '("" "-" "--"))
                        (v-pick '("" "0" "1" "3" "14" "100"))
                        (v-pick '("d" "w" "m" "y" "h" "x" ""))
                        (v-pick '("" "" " " ">" "x"))))
             (default (v-pick '(14 14 0 -3 7)))
             (delay (v-pick '(nil t)))
             (zero (v-pick '(nil t)))
             (org-deadline-warning-days default)
             (org-scheduled-delay-days default))
        (push (list s default (if delay 1 0) (if zero 1 0) (org-get-wdays s delay zero)) rows)))
    rows))

(defun v-time-text ()
  (mapconcat
   (lambda (_)
     (v-pick '("10:30" "9:05" "09:05" "3pm" "3PM" "12am" "12pm" "11:15am" "8:15pm" "24:00" "24:30" "34:00"
               "0:00" "7" "70" "10:3" "10:70" "10:300" "1:2" "am" "pm" "5am6" "x" "at" "-" "--" "---" ":" "."
               "$" "%" "'" "_" "é" "日" "(" ")" "<2026-10-06 Tue 14:00>" "<2026-10-06 Tue 14:00-15:30>"
               "<2026-10-06 Tue 9:00>" "<2026-10-06 Tue 14:00 +1w>" "<2026-10-06 14:00>" "<2026-10-06 Tue>"
               "[2026-10-06 Tue 14:00]" "<2026-10-06 Tue 16:00>" "<2026-10-07 Wed 16:00>" " " " " " " " ")))
   (number-sequence 1 (1+ (random 6))) ""))

(defun v-times ()
  (let (rows)
    (with-temp-buffer
      (org-mode)
      (dotimes (_ 6000)
        (let ((s (v-time-text)))
          (unless (string-match-p "[\t\n]" s)
            (push (list (concat "|" s "|")
                        (org-get-time-of-day s)
                        (and (string-match org-plain-time-of-day-regexp s)
                             (list (match-beginning 0) (match-end 0) (match-string 1 s) (match-string 8 s)))
                        (and (string-match org-stamp-time-of-day-regexp s)
                             (list (match-beginning 0) (match-end 0) (match-string 2 s)
                                   (match-string (if (match-end 3) 4 6) s))))
                  rows)))))
    rows))

(let ((dir (file-name-as-directory (car command-line-args-left))))
  (v-write (concat dir "closest-date.tsv") (v-closest-date))
  (v-write (concat dir "to-day.tsv") (v-to-day))
  (v-write (concat dir "wdays.tsv") (v-wdays))
  (v-write (concat dir "times.tsv") (v-times)))
